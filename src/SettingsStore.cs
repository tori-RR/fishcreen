using System;
using System.IO;

namespace SecondScreenDimmer
{
    internal static class SettingsStore
    {
        private static readonly string SettingsPath = AppPaths.InDataFolder("settings.txt");
        private static readonly string LegacySettingsPath = AppPaths.InLegacyDataFolder("settings.txt");

        internal static DimmerMode LoadMode()
        {
            try
            {
                string path = File.Exists(SettingsPath)
                    ? SettingsPath
                    : LegacySettingsPath;

                if (!File.Exists(path))
                {
                    return DimmerMode.Daily;
                }

                string text = File.ReadAllText(path).Trim();
                DimmerMode mode;
                if (Enum.TryParse(text, true, out mode) && Enum.IsDefined(typeof(DimmerMode), mode))
                {
                    return mode;
                }
            }
            catch (Exception exception)
            {
                AppLog.Write("Loading settings failed: " + exception.Message);
            }

            return DimmerMode.Daily;
        }

        internal static void SaveMode(DimmerMode mode)
        {
            try
            {
                Directory.CreateDirectory(AppPaths.DataFolder);
                File.WriteAllText(SettingsPath, mode.ToString());
            }
            catch (Exception exception)
            {
                AppLog.Write("Saving settings failed: " + exception.Message);
            }
        }
    }
}
