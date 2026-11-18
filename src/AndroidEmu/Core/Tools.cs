using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

namespace AndroidEmu.Core
{
    internal static class ToolPaths
    {
        public static string FindQemu(AppSettings settings) => Find(settings.QemuPath, "qemu-system-x86_64.exe",
            Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles), "qemu"));

        public static string FindAdb(AppSettings settings) => Find(settings.AdbPath, "adb.exe",
            SdkPlatformTools(Environment.GetEnvironmentVariable("ANDROID_HOME")),
            SdkPlatformTools(Environment.GetEnvironmentVariable("ANDROID_SDK_ROOT")),
            Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Android", "Sdk", "platform-tools"));

        private static string SdkPlatformTools(string path) => string.IsNullOrEmpty(path) ? null : Path.Combine(path, "platform-tools");

        private static string Find(string configured, string exe, params string[] folders)
        {
            if (!string.IsNullOrWhiteSpace(configured))
            {
                var path = ValidateExecutable(configured);
                if (!string.Equals(Path.GetFileName(path), exe, StringComparison.OrdinalIgnoreCase) &&
                    !(exe == "qemu-system-x86_64.exe" && string.Equals(Path.GetFileName(path), "qemu-system-x86_64w.exe", StringComparison.OrdinalIgnoreCase)))
                    throw new IOException("Укажите файл " + exe + " в настройках.");
                return path;
            }
            var searchPath = (Environment.GetEnvironmentVariable("PATH") ?? "").Split(Path.PathSeparator);
            foreach (var entry in folders.Concat(searchPath))
            {
                if (string.IsNullOrWhiteSpace(entry)) continue;
                var dir = entry.Trim().Trim('"');
                if (!Path.IsPathRooted(dir)) continue;
                var candidate = Path.Combine(dir, exe);
                if (File.Exists(candidate)) return Path.GetFullPath(candidate);
            }
            throw new FileNotFoundException(exe + " не найден. Укажите путь на вкладке «Настройки».");
        }

        public static string ValidateExecutable(string path)
        {
            var fullPath = Path.GetFullPath(path.Trim().Trim('"'));
            if (!string.Equals(Path.GetExtension(fullPath), ".exe", StringComparison.OrdinalIgnoreCase) || !File.Exists(fullPath))
                throw new FileNotFoundException("Программа не найдена: " + fullPath);
            return fullPath;
        }

        public static string FindQemuImg(string qemu)
        {
            var file = Path.Combine(Path.GetDirectoryName(qemu), "qemu-img.exe");
            if (!File.Exists(file))
                throw new FileNotFoundException("Для установки из ISO нужен qemu-img.exe рядом с QEMU: " + file);
            return file;
        }
    }

    internal static class CommandLine
    {
        // ProcessStartInfo.Arguments in .NET Framework is a single command line. Quote each
        // argument using the Windows CRT rules, without invoking cmd.exe or PowerShell.
        public static string Join(IEnumerable<string> args) => string.Join(" ", args.Select(Quote));

        public static string Quote(string argument)
        {
            if (argument.Length != 0 && !argument.Any(c => char.IsWhiteSpace(c) || c == '"'))
                return argument;
            var result = new StringBuilder("\"");
            int slashes = 0;
            foreach (var c in argument)
            {
                if (c == '\\') { slashes++; continue; }
                result.Append('\\', c == '"' ? 2 * slashes + 1 : slashes).Append(c);
                slashes = 0;
            }
            return result.Append('\\', 2 * slashes).Append('"').ToString();
        }

        public static string Number(int value) => value.ToString(CultureInfo.InvariantCulture);
    }

    internal sealed class CommandResult
    {
        public CommandResult(int exitCode, string output, string errors)
        {
            ExitCode = exitCode;
            Output = output;
            Errors = errors;
        }

        public int ExitCode { get; }
        public string Output { get; }
        public string Errors { get; }
        public string Details => (Output.Trim() + "\n" + Errors.Trim()).Trim();

        public void RequireSuccess(string operation)
        {
            if (ExitCode != 0)
                throw new IOException(operation + " (код " + ExitCode + ").\n" + Details);
        }
    }

    internal interface ICommandRunner
    {
        Task<CommandResult> RunAsync(string exe, string[] args, TimeSpan timeout, CancellationToken cancellation);
    }

    internal sealed class CommandRunner : ICommandRunner
    {
        public Task<CommandResult> RunAsync(string exe, string[] args, TimeSpan timeout, CancellationToken cancellation)
        {
            return Task.Run(async () =>
            {
                cancellation.ThrowIfCancellationRequested();
                using (var process = Process.Start(CreateStartInfo(exe, args)))
                using (cancellation.Register(() => TryKill(process)))
                {
                    var output = process.StandardOutput.ReadToEndAsync();
                    var errors = process.StandardError.ReadToEndAsync();
                    if (!process.WaitForExit((int)timeout.TotalMilliseconds))
                    {
                        TryKill(process);
                        process.WaitForExit(2000);
                        cancellation.ThrowIfCancellationRequested();
                        throw new TimeoutException(Path.GetFileName(exe) + " не ответил вовремя.");
                    }
                    cancellation.ThrowIfCancellationRequested();
                    return new CommandResult(process.ExitCode, await output.ConfigureAwait(false), await errors.ConfigureAwait(false));
                }
            }, cancellation);
        }

        public static ProcessStartInfo CreateStartInfo(string exe, IEnumerable<string> args) => new ProcessStartInfo
        {
            FileName = exe,
            Arguments = CommandLine.Join(args),
            WorkingDirectory = Path.GetDirectoryName(exe),
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            StandardOutputEncoding = Encoding.UTF8,
            StandardErrorEncoding = Encoding.UTF8
        };

        public static void TryKill(Process process)
        {
            try { if (!process.HasExited) process.Kill(); }
            catch (InvalidOperationException) { }
            catch (System.ComponentModel.Win32Exception) { }
        }
    }
}
