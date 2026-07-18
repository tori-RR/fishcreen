using System;
using System.IO;

namespace SecondScreenDimmer
{
    internal static class AppLog
    {
        private static readonly string LogPath = AppPaths.InDataFolder("app.log");

        internal static void Write(string message)
        {
            try
            {
                string folder = Path.GetDirectoryName(LogPath);
                Directory.CreateDirectory(folder);
                File.AppendAllText(
                    LogPath,
                    DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss") + " " + message + Environment.NewLine);
            }
            catch
            {
                // Logging must never prevent brightness recovery or application exit.
            }
        }
    }
}
