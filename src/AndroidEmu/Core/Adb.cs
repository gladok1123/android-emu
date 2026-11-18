using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text.RegularExpressions;
using System.Threading;
using System.Threading.Tasks;

namespace AndroidEmu.Core
{
    internal sealed class GuestAbi
    {
        public GuestAbi(IEnumerable<string> abis, string bridge)
        {
            Abis = abis.Where(ApkInfo.IsAbi).Distinct(StringComparer.Ordinal).ToArray();
            Bridge = string.IsNullOrWhiteSpace(bridge) || bridge.Trim() == "0" ? null : bridge.Trim();
        }

        public string[] Abis { get; }
        public string Bridge { get; }
        public bool HasArm64 => Abis.Contains("arm64-v8a");
        public bool HasArm32 => Abis.Contains("armeabi-v7a") || Abis.Contains("armeabi");
        public bool Accepts(IEnumerable<string> apkAbis)
        {
            var native = apkAbis.ToArray();
            return native.Length == 0 || native.Intersect(Abis, StringComparer.Ordinal).Any();
        }

        public string Describe() => "ABI Android: " + string.Join(", ", Abis) + "\n" +
            "ARM64 (arm64-v8a): " + (HasArm64 ? "доступно" : "нет") + "\n" +
            "ARM 32-bit: " + (HasArm32 ? "доступно" : "нет") + "\n" +
            "Native Bridge: " + (Bridge ?? "свойство не задано");
    }

    internal sealed class Adb
    {
        private static readonly TimeSpan ShortTimeout = TimeSpan.FromSeconds(8);
        private static readonly Regex Component = new Regex(@"\A[A-Za-z0-9_.$]+/[A-Za-z0-9_.$]+\z", RegexOptions.CultureInvariant);
        public const string ArmHint = "Для ARM APK нужен x86/x86_64 образ с активной трансляцией Native Bridge (libndk_translation или libhoudini). " +
            "Проверьте настройки ARM и поддерживаемые ABI выбранного образа. Сами трансляторы приложение не устанавливает.";

        private readonly string exe;
        private readonly ICommandRunner runner;
        public string Serial { get; }

        public Adb(string exe, int port, ICommandRunner runner)
        {
            if (port <= 0 || port > 65535) throw new ArgumentOutOfRangeException(nameof(port));
            this.exe = exe;
            this.runner = runner;
            Serial = "127.0.0.1:" + CommandLine.Number(port);
        }

        private Task<CommandResult> RunAsync(string[] args, CancellationToken cancellation, TimeSpan? timeout = null) =>
            runner.RunAsync(exe, new[] { "-s", Serial }.Concat(args).ToArray(), timeout ?? ShortTimeout, cancellation);

        public async Task ConnectAsync(CancellationToken cancellation)
        {
            // -P selects the ADB server port, not the device. The server stays on its
            // normal port; only the transport serial uses QEMU's forwarded port.
            var result = await runner.RunAsync(exe, new[] { "connect", Serial }, ShortTimeout, cancellation).ConfigureAwait(false);
            if (result.ExitCode != 0 || !Lines(result.Output).Any(s => s == "connected to " + Serial || s == "already connected to " + Serial))
                throw new IOException("ADB не подключился к Android (" + Serial + ").\n" + result.Details);
        }

        public async Task WaitForBootAsync(TimeSpan timeout, CancellationToken cancellation)
        {
            var clock = Stopwatch.StartNew();
            string details = "";
            while (clock.Elapsed < timeout)
            {
                cancellation.ThrowIfCancellationRequested();
                try
                {
                    await ConnectAsync(cancellation).ConfigureAwait(false);
                    if (await GetPropAsync("sys.boot_completed", cancellation).ConfigureAwait(false) == "1") return;
                }
                catch (Exception e) when (e is IOException || e is TimeoutException) { details = e.Message; }
                await Task.Delay(1500, cancellation).ConfigureAwait(false);
            }
            throw new TimeoutException("Android не ответил через ADB. Дождитесь загрузки, затем включите в госте " +
                "сетевую отладку ADB (TCP 5555) и подтвердите запрос авторизации, если он появится.\n" + details);
        }

        private async Task<string> GetPropAsync(string property, CancellationToken cancellation)
        {
            var result = await RunAsync(new[] { "shell", "getprop", property }, cancellation).ConfigureAwait(false);
            result.RequireSuccess("Не удалось прочитать " + property);
            return result.Output.Trim();
        }

        public async Task<GuestAbi> ReadAbiAsync(CancellationToken cancellation)
        {
            var list = await GetPropAsync("ro.product.cpu.abilist", cancellation).ConfigureAwait(false);
            if (string.IsNullOrWhiteSpace(list))
            {
                list = await GetPropAsync("ro.product.cpu.abi", cancellation).ConfigureAwait(false) + "," +
                    await GetPropAsync("ro.product.cpu.abi2", cancellation).ConfigureAwait(false);
            }
            var bridge = await GetPropAsync("ro.dalvik.vm.native.bridge", cancellation).ConfigureAwait(false);
            var status = new GuestAbi(list.Split(',').Select(s => s.Trim()), bridge);
            if (status.Abis.Length == 0) throw new IOException("Android не сообщил список поддерживаемых ABI.");
            return status;
        }

        public async Task InstallAsync(string path, ApkInfo apk, CancellationToken cancellation)
        {
            if (apk.Abis.Length != 0)
            {
                var guest = await ReadAbiAsync(cancellation).ConfigureAwait(false);
                if (!guest.Accepts(apk.Abis))
                    throw new InvalidDataException("ABI приложения и гостевой системы не совпадают.\n" +
                        "APK: " + string.Join(", ", apk.Abis) + "\n" + guest.Describe() + "\n\n" + ArmHint);
            }
            var result = await RunAsync(new[] { "install", "-r", Path.GetFullPath(path) }, cancellation, TimeSpan.FromMinutes(4)).ConfigureAwait(false);
            if (result.ExitCode != 0 || !Lines(result.Output).Contains("Success"))
                throw new IOException("Android не установил APK.\n" + result.Details +
                    (result.Details.Contains("INSTALL_FAILED_NO_MATCHING_ABIS") ? "\n\n" + ArmHint : ""));
        }

        public async Task<bool> LaunchAsync(string package, CancellationToken cancellation)
        {
            if (!ApkInfo.IsPackageName(package)) throw new ArgumentException("Недопустимое имя пакета.", nameof(package));
            var activity = await RunAsync(new[] { "shell", "cmd", "package", "resolve-activity", "--brief", "-a", "android.intent.action.MAIN",
                "-c", "android.intent.category.LAUNCHER", "-p", package }, cancellation).ConfigureAwait(false);
            activity.RequireSuccess("Не удалось найти запускаемый экран Android");
            var component = Lines(activity.Output).LastOrDefault(s => s.StartsWith(package + "/", StringComparison.Ordinal) && Component.IsMatch(s));
            if (component == null) return false;
            // Android uses its own shell after adb. Single quotes stop '$' from expanding
            // in an activity class name. The validated component contains no quotes.
            var start = await RunAsync(new[] { "shell", "am", "start", "-W", "-n", "'" + component + "'" }, cancellation,
                TimeSpan.FromSeconds(30)).ConfigureAwait(false);
            if (start.ExitCode != 0 || Lines(start.Details).Any(s => s.StartsWith("Error", StringComparison.OrdinalIgnoreCase) || s.StartsWith("Exception", StringComparison.OrdinalIgnoreCase)))
                throw new IOException("Не удалось открыть приложение.\n" + start.Details);
            return true;
        }

        public async Task<string[]> ListPackagesAsync(CancellationToken cancellation)
        {
            var result = await RunAsync(new[] { "shell", "pm", "list", "packages", "-3" }, cancellation).ConfigureAwait(false);
            result.RequireSuccess("Не удалось прочитать список приложений");
            return Lines(result.Output).Where(s => s.StartsWith("package:", StringComparison.Ordinal)).Select(s => s.Substring(8))
                .Where(ApkInfo.IsPackageName).OrderBy(s => s, StringComparer.Ordinal).ToArray();
        }

        private static IEnumerable<string> Lines(string output) => output.Split(new[] { '\r', '\n' }, StringSplitOptions.RemoveEmptyEntries).Select(s => s.Trim());
    }
}
