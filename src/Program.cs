using System;
using System.Threading;
using System.Windows.Forms;

namespace SecondScreenDimmer
{
    internal static class Program
    {
        private static TrayAppContext applicationContext;

        [STAThread]
        private static void Main()
        {
            bool createdNew;
            using (Mutex instanceMutex = new Mutex(
                true,
                "Local\\fishcreen-DELA0E7",
                out createdNew))
            {
                if (!createdNew)
                {
                    MessageBox.Show(
                        "fishcreen 已经在运行。",
                        "fishcreen",
                        MessageBoxButtons.OK,
                        MessageBoxIcon.Information);
                    return;
                }

                Application.EnableVisualStyles();
                Application.SetCompatibleTextRenderingDefault(false);
                Application.SetUnhandledExceptionMode(UnhandledExceptionMode.CatchException);

                Application.ThreadException += OnThreadException;
                AppDomain.CurrentDomain.UnhandledException += OnUnhandledException;

                applicationContext = new TrayAppContext();
                if (Array.Exists(
                    Environment.GetCommandLineArgs(),
                    delegate(string argument)
                    {
                        return string.Equals(
                            argument,
                            "--settings",
                            StringComparison.OrdinalIgnoreCase);
                    }))
                {
                    applicationContext.ShowSettingsForm();
                }

                Application.Run(applicationContext);
                GC.KeepAlive(instanceMutex);
            }
        }

        private static void OnThreadException(object sender, ThreadExceptionEventArgs eventArgs)
        {
            AppLog.Write("UI thread exception: " + eventArgs.Exception);
            RestoreAfterFailure();
            MessageBox.Show(
                "程序遇到错误，已尝试恢复副屏亮度。日志位于本地应用数据目录。",
                "fishcreen",
                MessageBoxButtons.OK,
                MessageBoxIcon.Error);
            Application.Exit();
        }

        private static void OnUnhandledException(object sender, UnhandledExceptionEventArgs eventArgs)
        {
            AppLog.Write("Unhandled exception: " + eventArgs.ExceptionObject);
            RestoreAfterFailure();
        }

        private static void RestoreAfterFailure()
        {
            if (applicationContext != null)
            {
                applicationContext.EmergencyRestore();
            }
        }
    }
}
