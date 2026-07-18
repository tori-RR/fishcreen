using System;
using System.Globalization;
using System.IO;

namespace SecondScreenDimmer
{
    internal static class RecoveryStore
    {
        private static readonly string RecoveryPath = AppPaths.InDataFolder(
            "brightness-recovery.txt");

        private static readonly string LegacyRecoveryPath = AppPaths.InLegacyDataFolder(
            "brightness-recovery.txt");

        internal static void Save(string stableId, uint brightness)
        {
            Directory.CreateDirectory(AppPaths.DataFolder);
            File.WriteAllLines(
                RecoveryPath,
                new string[]
                {
                    stableId,
                    brightness.ToString(CultureInfo.InvariantCulture)
                });
        }

        internal static bool TryLoad(string stableId, out uint brightness)
        {
            brightness = 0;

            try
            {
                string path = File.Exists(RecoveryPath)
                    ? RecoveryPath
                    : LegacyRecoveryPath;

                if (!File.Exists(path))
                {
                    return false;
                }

                string[] lines = File.ReadAllLines(path);
                if (lines.Length < 2 ||
                    !string.Equals(lines[0], stableId, StringComparison.OrdinalIgnoreCase))
                {
                    return false;
                }

                return uint.TryParse(
                    lines[1],
                    NumberStyles.Integer,
                    CultureInfo.InvariantCulture,
                    out brightness);
            }
            catch
            {
                return false;
            }
        }

        internal static void Clear()
        {
            try
            {
                if (File.Exists(RecoveryPath))
                {
                    File.Delete(RecoveryPath);
                }

                if (File.Exists(LegacyRecoveryPath))
                {
                    File.Delete(LegacyRecoveryPath);
                }
            }
            catch
            {
                // A stale recovery file is safer than losing the last known brightness.
            }
        }
    }
}
