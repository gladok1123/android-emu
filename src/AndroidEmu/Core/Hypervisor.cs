using System;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading.Tasks;

namespace AndroidEmu.Core
{
    internal enum HypervisorEnableResult { Enabled, RestartRequired, Cancelled }

    internal static class Hypervisor
    {
        public static readonly string[] EnableArguments = { "/Online", "/Enable-Feature", "/FeatureName:HypervisorPlatform", "/All", "/NoRestart" };

        [DllImport("WinHvPlatform.dll", ExactSpelling = true)]
        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        private static extern int WHvGetCapability(uint code, out int value, uint valueSize, out uint written);

        public static bool IsAvailable()
        {
            if (Environment.OSVersion.Platform != PlatformID.Win32NT) return false;
            try
            {
                // WHvCapabilityCodeHypervisorPresent (0), result is a Windows BOOL.
                return WHvGetCapability(0, out var present, sizeof(int), out var written) >= 0 &&
                    written >= sizeof(int) && present != 0;
            }
            catch (DllNotFoundException) { return false; }
            catch (EntryPointNotFoundException) { return false; }
        }

        public static Task<HypervisorEnableResult> EnableAsync() => Task.Run(() =>
        {
            // Elevate only the Windows servicing tool, not the app, QEMU, or scripts.
            var system = Environment.SystemDirectory;
            var args = new ProcessStartInfo
            {
                FileName = Path.Combine(system, "dism.exe"),
                WorkingDirectory = system,
                Arguments = CommandLine.Join(EnableArguments),
                UseShellExecute = true,
                Verb = "runas"
            };
            try
            {
                using (var process = Process.Start(args))
                {
                    if (process == null) throw new IOException("Не удалось запустить DISM.");
                    process.WaitForExit();
                    return InterpretExitCode(process.ExitCode);
                }
            }
            catch (Win32Exception e) when (e.NativeErrorCode == 1223)
            {
                return HypervisorEnableResult.Cancelled;
            }
        });

        public static HypervisorEnableResult InterpretExitCode(int code)
        {
            if (code == 0) return HypervisorEnableResult.Enabled;
            if (code == 3010) return HypervisorEnableResult.RestartRequired;
            throw new IOException("DISM не включил компонент. Код: " + code + " (0x" + code.ToString("X8") + ").\n" +
                "Проверьте журнал %WINDIR%\\Logs\\DISM\\dism.log и поддержку виртуализации в Windows.");
        }
    }
}
