using System;
using System.Collections.Generic;
using System.IO;
using System.IO.Compression;
using System.Linq;
using System.Text;
using System.Text.RegularExpressions;

namespace AndroidEmu.Core
{
    internal sealed class ApkInfo
    {
        private const int MaxManifestBytes = 2 * 1024 * 1024;
        private static readonly Regex PackagePattern = new Regex(@"\A[A-Za-z][A-Za-z0-9_]*(\.[A-Za-z][A-Za-z0-9_]*)*\z", RegexOptions.CultureInvariant);
        private static readonly Regex AbiPattern = new Regex(@"\A[A-Za-z0-9_-]+\z", RegexOptions.CultureInvariant);

        private ApkInfo(string package, string[] abis)
        {
            Package = package;
            Abis = abis;
        }

        public string Package { get; }
        public string[] Abis { get; }
        public static bool IsPackageName(string name) => name != null && name.Length <= 255 && PackagePattern.IsMatch(name);
        public static bool IsAbi(string abi) => abi.Length <= 32 && AbiPattern.IsMatch(abi);

        public static ApkInfo Read(string path)
        {
            if (!string.Equals(Path.GetExtension(path), ".apk", StringComparison.OrdinalIgnoreCase))
                throw new InvalidDataException("Выберите один файл .apk. Наборы .apks и .xapk не поддерживаются.");
            using (var file = File.OpenRead(path))
            using (var archive = new ZipArchive(file, ZipArchiveMode.Read))
            {
                var manifest = archive.GetEntry("AndroidManifest.xml");
                if (manifest == null || manifest.Length < 8 || manifest.Length > MaxManifestBytes)
                    throw new InvalidDataException("APK не содержит корректный AndroidManifest.xml.");
                var abis = new HashSet<string>(StringComparer.Ordinal);
                foreach (var entry in archive.Entries)
                {
                    if (!entry.FullName.StartsWith("lib/", StringComparison.Ordinal) ||
                        !entry.FullName.EndsWith(".so", StringComparison.Ordinal)) continue;
                    var lastSlash = entry.FullName.IndexOf('/', 4);
                    if (lastSlash <= 4 || entry.FullName.IndexOf('/', lastSlash + 1) != -1) continue;
                    var abi = entry.FullName.Substring(4, lastSlash - 4);
                    if (IsAbi(abi)) abis.Add(abi);
                }
                var bytes = new byte[(int)manifest.Length];
                using (var stream = manifest.Open())
                {
                    int read = 0;
                    while (read < bytes.Length)
                    {
                        var count = stream.Read(bytes, read, bytes.Length - read);
                        if (count == 0) throw new InvalidDataException("AndroidManifest.xml усечён.");
                        read += count;
                    }
                }
                var package = BinaryManifest.PackageName(bytes);
                if (!IsPackageName(package)) throw new InvalidDataException("Некорректное имя пакета APK.");
                return new ApkInfo(package, abis.OrderBy(a => a, StringComparer.Ordinal).ToArray());
            }
        }
    }

    // Only the package attribute is needed. Read AXML chunks and string pool without
    // extracting APK entries: no paths, files or code from an APK are executed on the host.
    internal sealed class BinaryManifest
    {
        private readonly byte[] bytes;
        private int poolOffset;
        private int poolHeader;
        private int poolCount;
        private int stringsOffset;
        private int stringsEnd;
        private bool utf8;

        private BinaryManifest(byte[] bytes) => this.bytes = bytes;
        public static string PackageName(byte[] bytes) => new BinaryManifest(bytes).ReadPackage();

        private string ReadPackage()
        {
            if (bytes.Length < 8 || U16(0) != 3) throw Invalid();
            var end = Integer(4);
            var begin = U16(2);
            if (begin < 8 || begin > end || end > bytes.Length) throw Invalid();
            for (int position = begin; position < end;)
            {
                if (end - position < 8) throw Invalid();
                var type = U16(position);
                var header = U16(position + 2);
                var size = Integer(position + 4);
                if (header < 8 || size < header || size > end - position) throw Invalid();
                if (type == 0x0001) ReadPool(position, header, size);
                if (type == 0x0102)
                {
                    if (poolCount == 0 || header < 16 || size - header < 20) throw Invalid();
                    var ext = position + header;
                    if (StringAt(Integer(ext + 4)) != "manifest") throw Invalid();
                    var start = U16(ext + 8);
                    var stride = U16(ext + 10);
                    var count = U16(ext + 12);
                    if (start < 20 || stride < 20 || start + (long)stride * count > size - header) throw Invalid();
                    for (int n = 0; n < count; n++)
                    {
                        var attr = ext + start + n * stride;
                        if (U32(attr) != uint.MaxValue || StringAt(Integer(attr + 4)) != "package") continue;
                        if (U32(attr + 8) != uint.MaxValue) return StringAt(Integer(attr + 8));
                        if (bytes[attr + 15] == 3) return StringAt(Integer(attr + 16));
                        throw Invalid();
                    }
                    throw Invalid();
                }
                position += size;
            }
            throw Invalid();
        }

        private void ReadPool(int offset, int header, int size)
        {
            if (header < 28 || poolOffset != 0) throw Invalid();
            poolCount = Integer(offset + 8);
            var stylesCount = Integer(offset + 12);
            var start = Integer(offset + 20);
            var stylesStart = Integer(offset + 24);
            if (start < header + 4L * (poolCount + (long)stylesCount) || start > size ||
                (stylesStart != 0 && (stylesStart < start || stylesStart > size))) throw Invalid();
            poolOffset = offset;
            poolHeader = header;
            stringsOffset = offset + start;
            stringsEnd = offset + (stylesStart == 0 ? size : stylesStart);
            utf8 = (U32(offset + 16) & 0x100) != 0;
        }

        private string StringAt(int index)
        {
            if (index >= poolCount) throw Invalid();
            var relative = Integer(poolOffset + poolHeader + index * 4);
            if (relative >= stringsEnd - stringsOffset) throw Invalid();
            var pos = stringsOffset + relative;
            if (utf8)
            {
                ReadLength8(ref pos); // UTF-16 code-unit count
                var count = ReadLength8(ref pos);
                if (count >= stringsEnd - pos || bytes[pos + count] != 0) throw Invalid();
                return new UTF8Encoding(false, true).GetString(bytes, pos, count);
            }
            if (stringsEnd - pos < 2) throw Invalid();
            var length = (int)U16(pos);
            pos += 2;
            if ((length & 0x8000) != 0)
            {
                if (stringsEnd - pos < 2) throw Invalid();
                length = ((length & 0x7fff) << 16) | U16(pos);
                pos += 2;
            }
            if (2L * length + 2 > stringsEnd - pos || U16(pos + 2 * length) != 0) throw Invalid();
            return new UnicodeEncoding(false, false, true).GetString(bytes, pos, 2 * length);
        }

        private int ReadLength8(ref int pos)
        {
            if (pos >= stringsEnd) throw Invalid();
            var n = (int)bytes[pos++];
            if ((n & 0x80) == 0) return n;
            if (pos >= stringsEnd) throw Invalid();
            return ((n & 0x7f) << 8) | bytes[pos++];
        }

        private ushort U16(int i) => (ushort)(bytes[i] | (bytes[i + 1] << 8));
        private uint U32(int i) => (uint)(U16(i) | (U16(i + 2) << 16));
        private int Integer(int i)
        {
            var n = U32(i);
            if (n > int.MaxValue) throw Invalid();
            return (int)n;
        }
        private static InvalidDataException Invalid() => new InvalidDataException("Не удалось прочитать имя пакета из AndroidManifest.xml.");
    }
}
