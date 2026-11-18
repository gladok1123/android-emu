using System;
using System.IO;
using System.Threading;
using System.Threading.Tasks;

namespace AndroidEmu.Core
{
    internal static class VirtualDisk
    {
        private static bool IsWindows => Path.DirectorySeparatorChar == '\\';

        // Accept a regular file path, never a raw device, network share, QEMU
        // block-device URI, or NTFS alternate stream.
        public static string NormalizePath(string path)
        {
            if (string.IsNullOrWhiteSpace(path) || (IsWindows ? !HasDriveRoot(path) : !Path.IsPathRooted(path)))
                throw new InvalidDataException("Выберите файл .qcow2 в папке на локальном диске.");
            if (!string.Equals(Path.GetExtension(path), ".qcow2", StringComparison.OrdinalIgnoreCase))
                throw new InvalidDataException("Имя виртуального диска должно оканчиваться на .qcow2.");
            var fullPath = Path.GetFullPath(path);
            var name = Path.GetFileName(fullPath);
            if (!string.Equals(Path.GetExtension(name), ".qcow2", StringComparison.OrdinalIgnoreCase) ||
                string.IsNullOrWhiteSpace(Path.GetFileNameWithoutExtension(name)) || name.IndexOfAny(Path.GetInvalidFileNameChars()) >= 0 ||
                (IsWindows && (!HasDriveRoot(fullPath) || IsDeviceName(name))))
                throw new InvalidDataException("Укажите допустимое имя файла .qcow2.");
            return fullPath;
        }

        private static bool HasDriveRoot(string path) => path.Length >= 3 &&
            ((path[0] >= 'A' && path[0] <= 'Z') || (path[0] >= 'a' && path[0] <= 'z')) &&
            path[1] == ':' && (path[2] == '\\' || path[2] == '/') && path.IndexOf(':', 2) < 0;

        private static bool IsDeviceName(string name)
        {
            int dot = name.IndexOf('.');
            var stem = (dot < 0 ? name : name.Substring(0, dot)).TrimEnd(' ').ToUpperInvariant();
            return stem == "CON" || stem == "PRN" || stem == "AUX" || stem == "NUL" ||
                stem == "CONIN$" || stem == "CONOUT$" ||
                (stem.Length == 4 && (stem.StartsWith("COM", StringComparison.Ordinal) || stem.StartsWith("LPT", StringComparison.Ordinal)) &&
                    "0123456789¹²³".IndexOf(stem[3]) >= 0);
        }

        private static void RequireLocalDrive(string path)
        {
            if (IsWindows && new DriveInfo(Path.GetPathRoot(path)).DriveType == DriveType.Network)
                throw new InvalidDataException("Сетевые диски не поддерживаются. Выберите локальную папку.");
        }

        public static string CheckDestination(string path)
        {
            var destination = NormalizePath(path);
            RequireLocalDrive(destination);
            if (!Directory.Exists(Path.GetDirectoryName(destination)))
                throw new DirectoryNotFoundException("Папка для виртуального диска не существует.");
            if (File.Exists(destination) || Directory.Exists(destination))
                throw new IOException("По этому пути уже есть файл или папка. Выберите другое имя: " + destination);
            return destination;
        }

        public static async Task CreateAsync(ICommandRunner runner, string qemuImg, string path, int sizeGiB, CancellationToken cancellation)
        {
            if (!AppSettings.ValidDiskSize(sizeGiB)) throw new ArgumentOutOfRangeException(nameof(sizeGiB));
            cancellation.ThrowIfCancellationRequested();
            var destination = CheckDestination(path);
            var staging = Path.Combine(Path.GetDirectoryName(destination), ".android-emu-" + Guid.NewGuid().ToString("N") + ".tmp");
            using (new FileStream(staging, FileMode.CreateNew, FileAccess.Write, FileShare.None)) { }
            try
            {
                // qemu-img can overwrite its output. It only sees our temporary file,
                // never the final name. File.Move does not replace an existing target.
                var result = await runner.RunAsync(qemuImg, new[] { "create", "-f", "qcow2", staging, CommandLine.Number(sizeGiB) + "G" },
                    TimeSpan.FromSeconds(60), cancellation).ConfigureAwait(false);
                result.RequireSuccess("Не удалось создать виртуальный диск");
                CheckStandaloneImage(staging);
                cancellation.ThrowIfCancellationRequested();
                File.Move(staging, destination);
            }
            catch
            {
                SettingsStore.TryDelete(staging);
                throw;
            }
        }

        public static string CheckForQemu(string path)
        {
            var fullPath = NormalizePath(path);
            RequireLocalDrive(fullPath);
            CheckStandaloneImage(fullPath);
            return fullPath;
        }

        private static void CheckStandaloneImage(string path)
        {
            if (!File.Exists(path)) throw new FileNotFoundException("Виртуальный диск не найден: " + path);
            if ((File.GetAttributes(path) & (FileAttributes.Device | FileAttributes.ReparsePoint)) != 0)
                throw new InvalidDataException("Виртуальный диск должен быть обычным файлом, а не ссылкой или устройством: " + path);
            var header = new byte[104];
            int read = 0;
            using (var file = File.OpenRead(path))
            {
                while (read < header.Length)
                {
                    int count = file.Read(header, read, header.Length - read);
                    if (count == 0) break;
                    read += count;
                }
            }
            if (read < 72 || header[0] != 'Q' || header[1] != 'F' || header[2] != 'I' || header[3] != 0xfb ||
                header[4] != 0 || header[5] != 0 || header[6] != 0 || (header[7] != 2 && header[7] != 3) ||
                (header[7] == 3 && read < header.Length))
                throw new InvalidDataException("Это не корректный файл QCOW2: " + path);
            // The backing file offset and size (bytes 8..19) must be zero. QCOW2 v3
            // may also redirect guest data to a separate file via incompatible bit 2.
            for (int i = 8; i < 20; i++)
                if (header[i] != 0) throw ExternalStorage(path);
            if (header[7] == 3 && (header[79] & 4) != 0) throw ExternalStorage(path);
        }

        private static InvalidDataException ExternalStorage(string path) =>
            new InvalidDataException("Поддерживается только отдельный диск QCOW2, без родительского образа или внешнего файла данных: " + path);
    }
}
