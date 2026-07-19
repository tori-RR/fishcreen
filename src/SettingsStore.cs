using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;

namespace SecondScreenDimmer
{
    internal static class SettingsStore
    {
        private static readonly string SettingsPath = AppPaths.InDataFolder("settings.txt");
        private static readonly string LegacySettingsPath = AppPaths.InLegacyDataFolder("settings.txt");

        internal static AppSettings Load()
        {
            AppSettings settings = new AppSettings();

            try
            {
                string path = File.Exists(SettingsPath)
                    ? SettingsPath
                    : LegacySettingsPath;

                if (!File.Exists(path))
                {
                    return settings;
                }

                string[] lines = File.ReadAllLines(path);
                if (lines.Length == 1 && lines[0].IndexOf('=') < 0)
                {
                    DimmerMode legacyMode;
                    if (Enum.TryParse(lines[0].Trim(), true, out legacyMode) &&
                        Enum.IsDefined(typeof(DimmerMode), legacyMode))
                    {
                        settings.Mode = legacyMode;
                    }

                    return settings;
                }

                Dictionary<string, string> values = new Dictionary<string, string>(
                    StringComparer.OrdinalIgnoreCase);

                foreach (string line in lines)
                {
                    int separator = line.IndexOf('=');
                    if (separator <= 0)
                    {
                        continue;
                    }

                    values[line.Substring(0, separator).Trim()] =
                        line.Substring(separator + 1).Trim();
                }

                string value;
                DimmerMode mode;
                AppThemeMode themeMode;
                int integerValue;
                double doubleValue;
                bool booleanValue;

                if (values.TryGetValue("TargetStableId", out value))
                {
                    settings.TargetStableId = value;
                }

                if (values.TryGetValue("Mode", out value) &&
                    Enum.TryParse(value, true, out mode) &&
                    Enum.IsDefined(typeof(DimmerMode), mode))
                {
                    settings.Mode = mode;
                }

                if (values.TryGetValue("ThemeMode", out value) &&
                    Enum.TryParse(value, true, out themeMode) &&
                    Enum.IsDefined(typeof(AppThemeMode), themeMode))
                {
                    settings.ThemeMode = themeMode;
                }

                if ((values.TryGetValue("CustomLeaveDelayMilliseconds", out value) ||
                    values.TryGetValue("LeaveDelayMilliseconds", out value)) &&
                    int.TryParse(value, NumberStyles.Integer, CultureInfo.InvariantCulture, out integerValue))
                {
                    settings.CustomProfile.LeaveDelayMilliseconds = integerValue;
                }

                if ((values.TryGetValue("CustomIdleBlackoutMilliseconds", out value) ||
                    values.TryGetValue("IdleBlackoutMilliseconds", out value)) &&
                    int.TryParse(value, NumberStyles.Integer, CultureInfo.InvariantCulture, out integerValue))
                {
                    settings.CustomProfile.IdleBlackoutMilliseconds = integerValue;
                }

                if ((values.TryGetValue("CustomBrightnessFadeEnabled", out value) ||
                    values.TryGetValue("BrightnessFadeEnabled", out value)) &&
                    bool.TryParse(value, out booleanValue))
                {
                    settings.CustomProfile.BrightnessFadeEnabled = booleanValue;
                }

                if ((values.TryGetValue("CustomBlackoutFadeEnabled", out value) ||
                    values.TryGetValue("BlackoutFadeEnabled", out value)) &&
                    bool.TryParse(value, out booleanValue))
                {
                    settings.CustomProfile.BlackoutFadeEnabled = booleanValue;
                }

                if (values.TryGetValue("CustomBrightnessFadeSteps", out value) &&
                    int.TryParse(value, NumberStyles.Integer, CultureInfo.InvariantCulture, out integerValue))
                {
                    settings.CustomProfile.BrightnessFadeSteps = integerValue;
                }

                if (values.TryGetValue("CustomBrightnessFadeExponent", out value) &&
                    double.TryParse(
                        value,
                        NumberStyles.Float,
                        CultureInfo.InvariantCulture,
                        out doubleValue))
                {
                    settings.CustomProfile.BrightnessFadeExponent = doubleValue;
                }
            }
            catch (Exception exception)
            {
                AppLog.Write("Loading settings failed: " + exception.Message);
            }

            settings.Normalize();
            return settings;
        }

        internal static void Save(AppSettings settings)
        {
            try
            {
                AppSettings normalized = settings.Clone();
                normalized.Normalize();

                Directory.CreateDirectory(AppPaths.DataFolder);
                File.WriteAllLines(
                    SettingsPath,
                    new string[]
                    {
                        "TargetStableId=" + normalized.TargetStableId,
                        "Mode=" + normalized.Mode,
                        "ThemeMode=" + normalized.ThemeMode,
                        "CustomLeaveDelayMilliseconds=" +
                            normalized.CustomProfile.LeaveDelayMilliseconds.ToString(
                            CultureInfo.InvariantCulture),
                        "CustomIdleBlackoutMilliseconds=" +
                            normalized.CustomProfile.IdleBlackoutMilliseconds.ToString(
                            CultureInfo.InvariantCulture),
                        "CustomBrightnessFadeEnabled=" +
                            normalized.CustomProfile.BrightnessFadeEnabled,
                        "CustomBlackoutFadeEnabled=" +
                            normalized.CustomProfile.BlackoutFadeEnabled,
                        "CustomBrightnessFadeSteps=" +
                            normalized.CustomProfile.BrightnessFadeSteps.ToString(
                            CultureInfo.InvariantCulture),
                        "CustomBrightnessFadeExponent=" +
                            normalized.CustomProfile.BrightnessFadeExponent.ToString(
                            "0.0",
                            CultureInfo.InvariantCulture)
                    });
            }
            catch (Exception exception)
            {
                AppLog.Write("Saving settings failed: " + exception.Message);
            }
        }
    }
}
