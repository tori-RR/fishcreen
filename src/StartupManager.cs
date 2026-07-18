using System;
using System.Windows.Forms;
using Microsoft.Win32;

namespace SecondScreenDimmer
{
    internal static class StartupManager
    {
        private const string RunKeyPath = @"Software\Microsoft\Windows\CurrentVersion\Run";
        private const string ValueName = "fishcreen";
        private const string LegacyValueName = "SecondScreenDimmer";

        internal static bool IsEnabled()
        {
            try
            {
                using (RegistryKey key = Registry.CurrentUser.OpenSubKey(RunKeyPath, false))
                {
                    if (key == null)
                    {
                        return false;
                    }

                    string configuredCommand = Convert.ToString(key.GetValue(ValueName));
                    if (string.IsNullOrWhiteSpace(configuredCommand))
                    {
                        return false;
                    }

                    string configuredPath = configuredCommand.Trim().Trim('"');
                    return string.Equals(
                        configuredPath,
                        Application.ExecutablePath,
                        StringComparison.OrdinalIgnoreCase);
                }
            }
            catch (Exception exception)
            {
                AppLog.Write("Reading startup registration failed: " + exception.Message);
                return false;
            }
        }

        internal static OperationResult SetEnabled(bool enabled)
        {
            try
            {
                using (RegistryKey key = Registry.CurrentUser.CreateSubKey(RunKeyPath))
                {
                    if (key == null)
                    {
                        return OperationResult.Fail("无法打开当前用户启动项");
                    }

                    if (enabled)
                    {
                        string command = "\"" + Application.ExecutablePath + "\"";
                        key.SetValue(ValueName, command, RegistryValueKind.String);
                        key.DeleteValue(LegacyValueName, false);
                        AppLog.Write("Startup registration enabled: " + command);
                        return OperationResult.Ok("已启用开机自动启动");
                    }

                    key.DeleteValue(ValueName, false);
                    key.DeleteValue(LegacyValueName, false);
                    AppLog.Write("Startup registration disabled.");
                    return OperationResult.Ok("已关闭开机自动启动");
                }
            }
            catch (Exception exception)
            {
                AppLog.Write("Changing startup registration failed: " + exception);
                return OperationResult.Fail("修改开机启动项失败");
            }
        }
    }
}
