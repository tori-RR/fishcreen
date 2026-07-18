using System;
using System.IO;

namespace SecondScreenDimmer
{
    internal static class AppPaths
    {
        internal static readonly string DataFolder = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "fishcreen");

        internal static readonly string LegacyDataFolder = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "SecondScreenDimmer");

        internal static string InDataFolder(string fileName)
        {
            return Path.Combine(DataFolder, fileName);
        }

        internal static string InLegacyDataFolder(string fileName)
        {
            return Path.Combine(LegacyDataFolder, fileName);
        }
    }
}
