using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Runtime.Serialization;
using System.Runtime.Serialization.Json;

namespace AndroidEmu.Core
{
    [DataContract]
    internal sealed class AndroidDisk
    {
        [DataMember] public string Id { get; set; }
        [DataMember] public string Name { get; set; }
        [DataMember] public int SizeGiB { get; set; }
        /// <summary>Null for disks stored in the app directory by v0.3.0.</summary>
        [DataMember(EmitDefaultValue = false)] public string FilePath { get; set; }

        public override string ToString() => Name + "  ·  " + SizeGiB + " ГиБ";
    }

    [DataContract]
    internal sealed class AppSettings
    {
        public const int MinMemory = 1024;
        public const int MaxMemory = 32768;
        public const int MaxCores = 16;
        public const int MinDiskSize = 8;
        public const int MaxDiskSize = 256;

        [DataMember] public string QemuPath { get; set; }
        [DataMember] public string AdbPath { get; set; }
        [DataMember] public int MemoryMb { get; set; }
        [DataMember] public int Cores { get; set; }
        [DataMember] public bool ForceSoftware { get; set; }
        [DataMember] public string ActiveDiskId { get; set; }
        [DataMember] public List<AndroidDisk> Disks { get; set; }

        public AppSettings() => SetDefaults();

        [OnDeserializing]
        private void OnDeserializing(StreamingContext _) => SetDefaults();

        private void SetDefaults()
        {
            QemuPath = "";
            AdbPath = "";
            MemoryMb = 4096;
            Cores = 2;
            Disks = new List<AndroidDisk>();
        }

        public AndroidDisk ActiveDisk => Disks.FirstOrDefault(d => d.Id == ActiveDiskId);

        public AppSettings Copy()
        {
            var copy = (AppSettings)MemberwiseClone();
            copy.Disks = new List<AndroidDisk>(Disks);
            return copy;
        }

        public void Validate()
        {
            if (MemoryMb < MinMemory || MemoryMb > MaxMemory || Cores < 1 || Cores > MaxCores)
                throw new InvalidDataException("Недопустимые значения RAM или числа ядер.");
            if (Disks == null || Disks.Any(d => d == null || !Guid.TryParseExact(d.Id, "N", out _) ||
                    string.IsNullOrWhiteSpace(d.Name) || !ValidDiskSize(d.SizeGiB)))
                throw new InvalidDataException("В настройках есть некорректная запись о диске Android.");
            foreach (var disk in Disks)
                if (disk.FilePath != null) VirtualDisk.NormalizePath(disk.FilePath);
            if (Disks.Select(d => d.Id).Distinct(StringComparer.OrdinalIgnoreCase).Count() != Disks.Count ||
                (ActiveDiskId != null && ActiveDisk == null))
                throw new InvalidDataException("Некорректный список дисков Android.");
        }

        public static bool ValidDiskSize(int size) => size >= MinDiskSize && size <= MaxDiskSize;
    }

    internal sealed class SettingsStore
    {
        private readonly string root;
        private readonly string settingsFile;

        public SettingsStore(string root)
        {
            this.root = Path.GetFullPath(root);
            settingsFile = Path.Combine(this.root, "settings.json");
        }

        public static string DefaultRoot => Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "android-emu", "winforms");

        public string LogsDirectory => Path.Combine(root, "logs");

        public AppSettings Load()
        {
            if (!File.Exists(settingsFile))
                return new AppSettings();
            try
            {
                using (var file = File.OpenRead(settingsFile))
                {
                    var settings = (AppSettings)new DataContractJsonSerializer(typeof(AppSettings)).ReadObject(file);
                    if (settings == null)
                        throw new InvalidDataException("Файл настроек пуст.");
                    settings.Validate();
                    return settings;
                }
            }
            catch (Exception e) when (e is IOException || e is InvalidDataException || e is UnauthorizedAccessException || e is SerializationException)
            {
                throw new IOException("Не удалось прочитать настройки: " + settingsFile + "\n" + e.Message, e);
            }
        }

        public void Save(AppSettings settings)
        {
            settings.Validate();
            Directory.CreateDirectory(root);
            var temp = settingsFile + "." + Guid.NewGuid().ToString("N") + ".tmp";
            try
            {
                using (var file = new FileStream(temp, FileMode.CreateNew, FileAccess.Write))
                {
                    new DataContractJsonSerializer(typeof(AppSettings)).WriteObject(file, settings);
                    file.Flush(true);
                }
                if (File.Exists(settingsFile))
                    File.Replace(temp, settingsFile, null);
                else
                    File.Move(temp, settingsFile);
            }
            finally
            {
                TryDelete(temp);
            }
        }

        public string DiskPath(AndroidDisk disk)
        {
            if (!Guid.TryParseExact(disk.Id, "N", out var id))
                throw new InvalidDataException("Некорректный идентификатор диска.");
            return disk.FilePath == null
                ? Path.Combine(root, "disks", id.ToString("N") + ".qcow2")
                : VirtualDisk.NormalizePath(disk.FilePath);
        }

        public static void TryDelete(string path)
        {
            try { File.Delete(path); }
            catch (IOException) { }
            catch (UnauthorizedAccessException) { }
        }
    }
}
