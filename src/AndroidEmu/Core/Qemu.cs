using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

namespace AndroidEmu.Core
{
    internal static class Qemu
    {
        public static string[] BuildArguments(AppSettings settings, string disk, string iso, int adbPort, bool whpx)
        {
            var args = new List<string>
            {
                "-name", iso == null ? "android-emu" : "android-emu: Android installer",
                "-machine", "q35",
                "-accel", whpx ? "whpx" : "tcg,thread=multi",
                // 'host' is not supported by WHPX in some QEMU releases. Nehalem provides
                // the SSE4.2/POPCNT baseline needed by many recent x86_64 Android builds.
                "-cpu", whpx ? "Nehalem" : "max",
                "-m", CommandLine.Number(settings.MemoryMb),
                "-smp", CommandLine.Number(settings.Cores),
                "-vga", "std",
                "-drive", "file=" + EscapeDrivePath(disk) + ",format=qcow2,if=ide,index=0",
                "-monitor", "none",
                "-serial", "none",
                "-qmp", "stdio"
            };
            if (iso != null)
            {
                args.AddRange(new[]
                {
                    "-drive", "file=" + EscapeDrivePath(iso) + ",format=raw,if=ide,index=2,media=cdrom,readonly=on",
                    "-boot", "order=c,once=d,menu=on",
                    "-nic", "none"
                });
            }
            else
            {
                if (adbPort <= 0 || adbPort > 65535) throw new ArgumentOutOfRangeException(nameof(adbPort));
                args.AddRange(new[]
                {
                    "-boot", "order=c",
                    "-netdev", "user,id=android,hostfwd=tcp:127.0.0.1:" + CommandLine.Number(adbPort) + "-:5555",
                    "-device", "e1000,netdev=android"
                });
            }
            return args.ToArray();
        }

        private static string EscapeDrivePath(string path) => path.Replace(",", ",,");

        public static int AllocateAdbPort()
        {
            var listener = new TcpListener(IPAddress.Loopback, 0);
            listener.Start();
            try { return ((IPEndPoint)listener.LocalEndpoint).Port; }
            finally { listener.Stop(); }
        }

        public static async Task<VmSession> StartAsync(string qemu, AppSettings settings, string disk,
            string iso, string logsDirectory, CancellationToken cancellation)
        {
            disk = VirtualDisk.CheckForQemu(disk);
            Directory.CreateDirectory(logsDirectory);
            var log = Path.Combine(logsDirectory, (iso == null ? "android-" : "install-") +
                DateTime.Now.ToString("yyyyMMdd-HHmmss") + "-" + Guid.NewGuid().ToString("N").Substring(0, 8) + ".log");
            var adbPort = iso == null ? AllocateAdbPort() : 0;
            if (!settings.ForceSoftware && Hypervisor.IsAvailable())
            {
                try { return await StartOnceAsync(qemu, settings, disk, iso, adbPort, true, log, cancellation).ConfigureAwait(false); }
                catch (QemuStartupException)
                {
                    File.AppendAllText(log, "\nWHPX startup failed; retrying with TCG.\n");
                }
            }
            return await StartOnceAsync(qemu, settings, disk, iso, adbPort, false, log, cancellation).ConfigureAwait(false);
        }

        private static async Task<VmSession> StartOnceAsync(string qemu, AppSettings settings, string disk,
            string iso, int port, bool whpx, string log, CancellationToken cancellation)
        {
            cancellation.ThrowIfCancellationRequested();
            var args = BuildArguments(settings, disk, iso, port, whpx);
            File.AppendAllText(log, "\n" + qemu + " " + CommandLine.Join(args) + "\n");
            var vm = new VmSession(qemu, args, port, whpx, log);
            try
            {
                await Task.WhenAny(vm.Exit, Task.Delay(1200, cancellation)).ConfigureAwait(false);
                cancellation.ThrowIfCancellationRequested();
                if (vm.Exit.IsCompleted)
                {
                    await vm.DrainOutputAsync().ConfigureAwait(false);
                    throw new QemuStartupException("QEMU завершился сразу после запуска (код " + await vm.Exit.ConfigureAwait(false) +
                        ").\n" + vm.LastError + "\nЖурнал: " + log);
                }
                return vm;
            }
            catch
            {
                await vm.StopAsync(force: true).ConfigureAwait(false);
                vm.Dispose();
                throw;
            }
        }

        private sealed class QemuStartupException : IOException
        {
            public QemuStartupException(string message) : base(message) { }
        }
    }

    internal sealed class VmSession : IDisposable
    {
        private readonly Process process;
        private readonly TaskCompletionSource<int> exited = new TaskCompletionSource<int>(TaskCreationOptions.RunContinuationsAsynchronously);
        private readonly Task outputEnded;
        private readonly Task errorsEnded;
        private readonly object logLock = new object();
        private string lastError = "";
        private bool stopRequested;

        public VmSession(string qemu, string[] args, int port, bool whpx, string log)
        {
            LogPath = log;
            AdbPort = port;
            HasWhpx = whpx;
            var info = CommandRunner.CreateStartInfo(qemu, args);
            info.RedirectStandardInput = true;
            process = new Process { StartInfo = info, EnableRaisingEvents = true };
            process.Exited += (_, __) => exited.TrySetResult(process.ExitCode);
            var outputFinished = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
            var errorsFinished = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
            process.OutputDataReceived += (_, e) => { if (e.Data == null) outputFinished.TrySetResult(true); };
            process.ErrorDataReceived += (_, e) =>
            {
                if (e.Data == null) errorsFinished.TrySetResult(true);
                else LogError(e.Data);
            };
            try
            {
                process.Start();
                process.BeginOutputReadLine();
                process.BeginErrorReadLine();
            }
            catch
            {
                CommandRunner.TryKill(process);
                process.Dispose();
                throw;
            }
            outputEnded = outputFinished.Task;
            errorsEnded = errorsFinished.Task;
        }

        public int AdbPort { get; }
        public bool HasWhpx { get; }
        public string LogPath { get; }
        public Task<int> Exit => exited.Task;
        public string LastError { get { lock (logLock) return lastError; } }
        public Task DrainOutputAsync() => Task.WhenAny(Task.WhenAll(outputEnded, errorsEnded), Task.Delay(2000));

        private void LogError(string line)
        {
            lock (logLock)
            {
                if (!string.IsNullOrWhiteSpace(line)) lastError = line;
                try { File.AppendAllText(LogPath, line + Environment.NewLine, Encoding.UTF8); }
                catch (IOException) { }
                catch (UnauthorizedAccessException) { }
            }
        }

        public async Task<bool> StopAsync(bool force)
        {
            if (Exit.IsCompleted) return true;
            if (force)
            {
                CommandRunner.TryKill(process);
                await Task.WhenAny(Exit, Task.Delay(4000)).ConfigureAwait(false);
                return Exit.IsCompleted;
            }
            if (!stopRequested)
            {
                stopRequested = true;
                try
                {
                    await process.StandardInput.WriteLineAsync("{\"execute\":\"qmp_capabilities\"}").ConfigureAwait(false);
                    await process.StandardInput.WriteLineAsync("{\"execute\":\"system_powerdown\"}").ConfigureAwait(false);
                    await process.StandardInput.FlushAsync().ConfigureAwait(false);
                }
                catch (IOException) { }
                catch (InvalidOperationException) { }
            }
            await Task.WhenAny(Exit, Task.Delay(TimeSpan.FromSeconds(10))).ConfigureAwait(false);
            if (!Exit.IsCompleted) stopRequested = false;
            return Exit.IsCompleted;
        }

        public void Dispose() => process.Dispose();
    }

    internal static class IsoInstaller
    {
        public static void CheckIso(string iso)
        {
            if (!string.Equals(Path.GetExtension(iso), ".iso", StringComparison.OrdinalIgnoreCase))
                throw new InvalidDataException("Выберите установочный файл .iso для ПК (x86 или x86_64).\n" +
                    "Образы ARM, IMG телефонов и системные образы AVD не подходят.");
            using (var file = File.OpenRead(iso))
            {
                var signature = new byte[5];
                if (file.Length < 16 * 2048 + 6)
                    throw new InvalidDataException("Файл слишком мал для установочного ISO.");
                file.Position = 16 * 2048 + 1;
                if (file.Read(signature, 0, signature.Length) != signature.Length || Encoding.ASCII.GetString(signature) != "CD001")
                    throw new InvalidDataException("Не найден заголовок ISO 9660. Нужен загрузочный ISO для ПК.");
            }
        }
    }
}
