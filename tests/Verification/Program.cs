using System;
using System.Collections.Generic;
using System.IO;
using System.IO.Compression;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using AndroidEmu.Core;

namespace AndroidEmu.Verification
{
    internal static class Program
    {
        private static int passed;
        private static string root;

        private static int Main(string[] args)
        {
            Console.OutputEncoding = Encoding.UTF8;
            root = Path.Combine(Path.GetTempPath(), "android-emu-test-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(root);
            try
            {
                if (args.Length > 0)
                {
                    foreach (var path in args)
                    {
                        var apk = ApkInfo.Read(path);
                        Console.WriteLine(Path.GetFileName(path) + ": " + apk.Package + " [" + string.Join(", ", apk.Abis) + "]");
                    }
                    return 0;
                }
                Test("settings round-trip and disk paths", SettingsRoundTrip);
                Test("ISO validation", ValidateIso);
                Test("install versus normal boot QEMU arguments", QemuArguments);
                Test("safe QCOW2 allocation and cleanup", () => DiskCreation().GetAwaiter().GetResult());
                Test("Windows argument quoting", WindowsQuoting);
                Test("APK manifest and ABI inspection", ApkMetadata);
                Test("guest ABI matching", AbiMatching);
                Test("ADB transport, native APK preflight and launch", () => AdbFlow().GetAwaiter().GetResult());
                Test("ADB rejects bad connection and incompatible apps", () => AdbFailures().GetAwaiter().GetResult());
                Test("DISM options and reboot result", HypervisorOptions);
                Console.WriteLine(passed + " verification cases passed.");
                return 0;
            }
            catch (Exception e)
            {
                Console.Error.WriteLine(e);
                return 1;
            }
            finally { Directory.Delete(root, recursive: true); }
        }

        private static void Test(string name, Action test)
        {
            test();
            passed++;
            Console.WriteLine("PASS " + name);
        }

        private static void SettingsRoundTrip()
        {
            var store = new SettingsStore(Path.Combine(root, "settings"));
            var settings = store.Load();
            Require(settings.MemoryMb == 4096 && settings.ActiveDisk == null && !settings.ForceSoftware, "Unexpected defaults");
            var disk = new AndroidDisk { Name = "Система", SizeGiB = 32, Id = Guid.NewGuid().ToString("N") };
            settings.Disks.Add(disk);
            settings.ActiveDiskId = disk.Id;
            settings.ForceSoftware = true;
            settings.Cores = 4;
            store.Save(settings);
            settings.MemoryMb = 6144;
            store.Save(settings);
            var loaded = store.Load();
            Require(loaded.ActiveDisk.Name == disk.Name && loaded.MemoryMb == 6144 && loaded.Cores == 4 && loaded.ForceSoftware, "Settings did not persist");
            Require(store.DiskPath(disk).EndsWith(disk.Id + ".qcow2", StringComparison.Ordinal), "Wrong disk path");
            Throws<InvalidDataException>(() => store.DiskPath(new AndroidDisk { Id = "../another-file" }));
            loaded.ActiveDiskId = Guid.NewGuid().ToString("N");
            Throws<InvalidDataException>(() => store.Save(loaded));
            Require(store.Load().ActiveDisk.Id == disk.Id, "Failed save changed settings");
        }

        private static void ValidateIso()
        {
            var path = Path.Combine(root, "Android Test, 64.ISO");
            var payload = new byte[17 * 2048];
            Encoding.ASCII.GetBytes("CD001").CopyTo(payload, 16 * 2048 + 1);
            File.WriteAllBytes(path, payload);
            IsoInstaller.CheckIso(path);
            Throws<InvalidDataException>(() => IsoInstaller.CheckIso(Path.Combine(root, "system.img")));
            File.WriteAllBytes(path, new byte[17 * 2048]);
            Throws<InvalidDataException>(() => IsoInstaller.CheckIso(path));
        }

        private static void QemuArguments()
        {
            var settings = new AppSettings { MemoryMb = 6144, Cores = 4 };
            var disk = @"C:\Disk Images\Android, 64\guest.qcow2";
            var iso = @"C:\Disk Images\Bliss, 64.iso";
            var install = Qemu.BuildArguments(settings, disk, iso, 0, true);
            Require(Option(install, "-accel") == "whpx" && Option(install, "-cpu") != "host", "Wrong WHPX accelerator");
            Require(Option(install, "-m") == "6144" && Option(install, "-smp") == "4", "Settings not applied to QEMU");
            Require(install.Contains("file=" + disk.Replace(",", ",,") + ",format=qcow2,if=ide,index=0"), "Wrong destination arguments");
            Require(install.Contains("file=" + iso.Replace(",", ",,") + ",format=raw,if=ide,index=2,media=cdrom,readonly=on"), "ISO not attached read-only");
            Require(Option(install, "-boot") == "order=c,once=d,menu=on" && Option(install, "-nic") == "none" && !install.Contains("-netdev"), "Installer not isolated");
            var normal = Qemu.BuildArguments(settings, disk, null, 5601, false);
            Require(normal.Count(x => x == "-drive") == 1 && Option(normal, "-boot") == "order=c", "Normal boot includes installer media");
            Require(Option(normal, "-accel") == "tcg,thread=multi", "TCG fallback missing");
            Require(Option(normal, "-netdev").Contains("hostfwd=tcp:127.0.0.1:5601-:5555"), "ADB forward is not loopback-only");
            Require(Option(normal, "-qmp") == "stdio" && Option(normal, "-monitor") == "none", "QEMU management not private");
        }

        private static async Task DiskCreation()
        {
            var dest = Path.Combine(root, "disks", "d.qcow2");
            var runner = new FakeRunner((exe, args) =>
            {
                Require(exe == "qemu-img.exe" && args.SequenceEqual(new[] { "create", "-f", "qcow2", dest, "32G" }), "Unexpected qemu-img args");
                File.WriteAllBytes(dest, new byte[] { 1, 2, 3 });
                return Ok();
            });
            await IsoInstaller.CreateDiskAsync(runner, "qemu-img.exe", dest, 32, CancellationToken.None);
            await ThrowsAsync<IOException>(() => IsoInstaller.CreateDiskAsync(runner, "qemu-img.exe", dest, 32, CancellationToken.None));
            Require(runner.Calls.Count == 1 && File.ReadAllBytes(dest).SequenceEqual(new byte[] { 1, 2, 3 }), "Existing disk overwritten");
            var failedDest = Path.Combine(root, "disks", "failed.qcow2");
            var failed = new FakeRunner((exe, args) => new CommandResult(1, "", "no space"));
            await ThrowsAsync<IOException>(() => IsoInstaller.CreateDiskAsync(failed, "qemu-img.exe", failedDest, 32, CancellationToken.None));
            Require(!File.Exists(failedDest), "Partial disk left after failure");
        }

        private static void WindowsQuoting()
        {
            Require(CommandLine.Quote("") == "\"\"" && CommandLine.Quote("whpx") == "whpx", "Empty/simple argument lost");
            Require(CommandLine.Quote(@"C:\Disk Images\") == "\"C:\\Disk Images\\\\\"", "Trailing backslash not escaped");
            Require(CommandLine.Quote("a\"b") == "\"a\\\"b\"", "Embedded quote not escaped");
        }

        private static void ApkMetadata()
        {
            var arm = ApkInfo.Read(CreateApk("arm.apk", Manifest("com.test.arm", true, false), "arm64-v8a", "armeabi-v7a"));
            Require(arm.Package == "com.test.arm" && arm.Abis.SequenceEqual(new[] { "arm64-v8a", "armeabi-v7a" }), "ARM APK misclassified");
            var java = ApkInfo.Read(CreateApk("java.apk", Manifest("com.test.jvm", false, true)));
            Require(java.Package == "com.test.jvm" && java.Abis.Length == 0, "Java APK misclassified");
            Throws<InvalidDataException>(() => ApkInfo.Read(CreateApk("bad-name.apk", Manifest("com.test.app;id", true, false))));
            Throws<InvalidDataException>(() => BinaryManifest.PackageName(new byte[] { 3, 0, 8, 0, 8, 0, 0, 0 }));
            var mangled = Manifest("com.test.arm", true, false);
            mangled[16] = 0xff;
            mangled[17] = 0xff;
            Throws<InvalidDataException>(() => BinaryManifest.PackageName(mangled));
        }

        private static void AbiMatching()
        {
            var guest = new GuestAbi(new[] { "x86_64", "arm64-v8a" }, "libndk_translation.so");
            Require(guest.HasArm64 && !guest.HasArm32 && guest.Accepts(new[] { "arm64-v8a" }), "ARM64 readiness incorrect");
            Require(!guest.Accepts(new[] { "armeabi-v7a" }), "ARM32 incorrectly accepted on ARM64-only guest");
            Require(guest.Accepts(new[] { "x86_64", "armeabi-v7a" }) && guest.Accepts(new string[0]), "Universal or Java APK blocked");
            var notEnabled = new GuestAbi(new[] { "x86" }, "libhoudini.so");
            Require(!notEnabled.HasArm32 && !notEnabled.Accepts(new[] { "armeabi" }), "Library name falsely implies ARM support");
        }

        private static async Task AdbFlow()
        {
            var path = CreateApk("guest-app.apk", Manifest("com.test.run", true, false), "arm64-v8a");
            var apk = ApkInfo.Read(path);
            var runner = new FakeRunner((exe, args) =>
            {
                if (args[0] == "connect")
                {
                    Require(args.SequenceEqual(new[] { "connect", "127.0.0.1:5603" }), "Connect targets wrong serial");
                    return Ok("already connected to 127.0.0.1:5603\n");
                }
                Require(args[0] == "-s" && args[1] == "127.0.0.1:5603", "ADB command not bound to guest transport");
                if (args.Contains("sys.boot_completed")) return Ok("1\n");
                if (args.Contains("ro.product.cpu.abilist")) return Ok("x86_64,arm64-v8a\n");
                if (args.Contains("ro.dalvik.vm.native.bridge")) return Ok("libndk_translation.so\n");
                if (args.Contains("install")) { Require(args.Last() == path, "APK path changed"); return Ok("Performing Streamed Install\nSuccess\n"); }
                if (args.Contains("resolve-activity")) return Ok("priority=0\ncom.test.run/.Main$Activity\n");
                Require(args.Last() == "'com.test.run/.Main$Activity'", "Activity not quoted for the guest shell");
                return Ok("Status: ok\n");
            });
            var adb = new Adb("adb.exe", 5603, runner);
            await adb.WaitForBootAsync(TimeSpan.FromSeconds(2), CancellationToken.None);
            await adb.InstallAsync(path, apk, CancellationToken.None);
            Require(await adb.LaunchAsync(apk.Package, CancellationToken.None), "Launcher unexpectedly absent");
            Require(runner.Calls.FindIndex(a => a.Contains("ro.product.cpu.abilist")) < runner.Calls.FindIndex(a => a.Contains("install")), "ARM check ran after installation");
        }

        private static async Task AdbFailures()
        {
            var disconnected = new FakeRunner((exe, args) => Ok("not connected to 127.0.0.1:5603"));
            await ThrowsAsync<IOException>(() => new Adb("adb.exe", 5603, disconnected).ConnectAsync(CancellationToken.None));
            var path = CreateApk("arm32.apk", Manifest("com.test.arm32", true, false), "armeabi-v7a");
            var runner = new FakeRunner((exe, args) =>
            {
                Require(!args.Contains("install"), "Incompatible APK was sent to Android");
                return Ok(args.Contains("ro.product.cpu.abilist") ? "x86_64,arm64-v8a" : "libndk_translation.so");
            });
            await ThrowsAsync<InvalidDataException>(() => new Adb("adb.exe", 5603, runner).InstallAsync(path, ApkInfo.Read(path), CancellationToken.None));
        }

        private static void HypervisorOptions()
        {
            Require(Hypervisor.EnableArguments.Count(a => a.StartsWith("/FeatureName:")) == 1 &&
                Hypervisor.EnableArguments.Contains("/FeatureName:HypervisorPlatform") && Hypervisor.EnableArguments.Contains("/NoRestart"), "Wrong Windows feature or missing /NoRestart");
            Require(Hypervisor.InterpretExitCode(0) == HypervisorEnableResult.Enabled, "Wrong success result");
            Require(Hypervisor.InterpretExitCode(3010) == HypervisorEnableResult.RestartRequired, "Reboot requirement lost");
            Throws<IOException>(() => Hypervisor.InterpretExitCode(5));
        }

        private static byte[] Manifest(string package, bool utf8, bool typed)
        {
            var strings = new[] { "manifest", "package", package };
            using (var file = new MemoryStream())
            using (var writer = new BinaryWriter(file))
            {
                var records = strings.Select(s => utf8
                    ? new[] { (byte)s.Length, (byte)Encoding.UTF8.GetByteCount(s) }.Concat(Encoding.UTF8.GetBytes(s)).Concat(new byte[1]).ToArray()
                    : BitConverter.GetBytes((ushort)s.Length).Concat(Encoding.Unicode.GetBytes(s)).Concat(new byte[2]).ToArray()).ToArray();
                int chars = records.Sum(x => x.Length);
                int padded = (chars + 3) & ~3;
                writer.Write((ushort)3); writer.Write((ushort)8); writer.Write(8 + 40 + padded + 56);
                writer.Write((ushort)1); writer.Write((ushort)28); writer.Write(40 + padded);
                writer.Write(3); writer.Write(0); writer.Write(utf8 ? 0x100 : 0); writer.Write(40); writer.Write(0);
                int offset = 0;
                foreach (var entry in records) { writer.Write(offset); offset += entry.Length; }
                foreach (var entry in records) writer.Write(entry);
                writer.Write(new byte[padded - chars]);
                writer.Write((ushort)0x0102); writer.Write((ushort)16); writer.Write(56);
                writer.Write(1); writer.Write(-1); // line/comment
                writer.Write(-1); writer.Write(0); // node namespace/name
                writer.Write((ushort)20); writer.Write((ushort)20); writer.Write((ushort)1);
                writer.Write((ushort)0); writer.Write((ushort)0); writer.Write((ushort)0);
                writer.Write(-1); writer.Write(1); writer.Write(typed ? -1 : 2);
                writer.Write((ushort)8); writer.Write((byte)0); writer.Write((byte)3); writer.Write(2);
                return file.ToArray();
            }
        }

        private static string CreateApk(string name, byte[] manifest, params string[] abis)
        {
            var path = Path.Combine(root, name);
            using (var file = File.Create(path))
            using (var zip = new ZipArchive(file, ZipArchiveMode.Create))
            {
                using (var entry = zip.CreateEntry("AndroidManifest.xml").Open()) entry.Write(manifest, 0, manifest.Length);
                foreach (var abi in abis) zip.CreateEntry("lib/" + abi + "/libtest.so");
            }
            return path;
        }

        private static string Option(string[] args, string flag) => args[Array.IndexOf(args, flag) + 1];
        private static CommandResult Ok(string output = "") => new CommandResult(0, output, "");
        private static void Require(bool condition, string message) { if (!condition) throw new Exception(message); }
        private static void Throws<T>(Action action) where T : Exception
        {
            try { action(); }
            catch (T) { return; }
            throw new Exception("Expected " + typeof(T).Name);
        }
        private static async Task ThrowsAsync<T>(Func<Task> action) where T : Exception
        {
            try { await action(); }
            catch (T) { return; }
            throw new Exception("Expected " + typeof(T).Name);
        }

        private sealed class FakeRunner : ICommandRunner
        {
            private readonly Func<string, string[], CommandResult> run;
            public readonly List<string[]> Calls = new List<string[]>();
            public FakeRunner(Func<string, string[], CommandResult> run) => this.run = run;
            public Task<CommandResult> RunAsync(string exe, string[] args, TimeSpan timeout, CancellationToken cancellation)
            {
                cancellation.ThrowIfCancellationRequested();
                Calls.Add(args);
                return Task.FromResult(run(exe, args));
            }
        }
    }
}
