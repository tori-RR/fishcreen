using System;
using System.Drawing;
using System.Windows.Forms;
using Microsoft.Win32;

namespace SecondScreenDimmer
{
    internal sealed class TrayAppContext : ApplicationContext
    {
        private readonly ScreenDimmerController controller;
        private readonly NotifyIcon trayIcon;
        private readonly Icon appIcon;
        private readonly ToolStripMenuItem dailyModeItem;
        private readonly ToolStripMenuItem movieModeItem;
        private readonly ToolStripMenuItem customModeItem;
        private readonly ToolStripMenuItem offModeItem;
        private readonly ToolStripMenuItem startupItem;
        private readonly ToolStripMenuItem statusItem;
        private SettingsForm settingsForm;
        private bool changingStartupItem;
        private bool exiting;

        internal TrayAppContext()
        {
            controller = new ScreenDimmerController();
            controller.StatusChanged += OnStatusChanged;
            controller.ModeChanged += OnModeChanged;

            dailyModeItem = new ToolStripMenuItem("日常模式");
            dailyModeItem.Click += delegate { controller.SetMode(DimmerMode.Daily); };

            movieModeItem = new ToolStripMenuItem("观影模式");
            movieModeItem.Click += delegate { controller.SetMode(DimmerMode.Movie); };

            customModeItem = new ToolStripMenuItem("自定义");
            customModeItem.Click += delegate { controller.SetMode(DimmerMode.Custom); };

            offModeItem = new ToolStripMenuItem("关闭自动暗屏");
            offModeItem.Click += delegate { controller.SetMode(DimmerMode.Off); };

            ToolStripMenuItem modeMenu = new ToolStripMenuItem("运行模式");
            modeMenu.DropDownItems.Add(dailyModeItem);
            modeMenu.DropDownItems.Add(movieModeItem);
            modeMenu.DropDownItems.Add(customModeItem);
            modeMenu.DropDownItems.Add(offModeItem);

            ToolStripMenuItem settingsItem = new ToolStripMenuItem("设置…");
            settingsItem.Click += delegate { ShowSettingsForm(); };

            startupItem = new ToolStripMenuItem("开机自动启动");
            startupItem.CheckOnClick = true;
            startupItem.Checked = StartupManager.IsEnabled();
            startupItem.CheckedChanged += OnStartupCheckedChanged;

            statusItem = new ToolStripMenuItem("正在启动…");
            statusItem.Enabled = false;

            ToolStripMenuItem restoreItem = new ToolStripMenuItem("立即恢复副屏");
            restoreItem.Click += delegate { controller.ForceRestore(); };

            ToolStripMenuItem exitItem = new ToolStripMenuItem("退出");
            exitItem.Click += delegate { ExitApplication(); };

            ContextMenuStrip menu = new ContextMenuStrip();
            menu.Items.Add(settingsItem);
            menu.Items.Add(new ToolStripSeparator());
            menu.Items.Add(modeMenu);
            menu.Items.Add(startupItem);
            menu.Items.Add(statusItem);
            menu.Items.Add(new ToolStripSeparator());
            menu.Items.Add(restoreItem);
            menu.Items.Add(new ToolStripSeparator());
            menu.Items.Add(exitItem);
            menu.Opening += delegate
            {
                ThemeManager.ApplyContextMenuTheme(
                    menu,
                    controller.GetSettingsSnapshot().ThemeMode);
            };
            ThemeManager.ApplyContextMenuTheme(
                menu,
                controller.GetSettingsSnapshot().ThemeMode);

            trayIcon = new NotifyIcon();
            appIcon = LoadApplicationIcon();
            trayIcon.Icon = appIcon ?? SystemIcons.Application;
            trayIcon.Text = "fishcreen";
            trayIcon.ContextMenuStrip = menu;
            trayIcon.Visible = true;
            trayIcon.MouseClick += delegate(object sender, MouseEventArgs eventArgs)
            {
                if (eventArgs.Button == MouseButtons.Left)
                {
                    ShowSettingsForm();
                }
            };

            SystemEvents.DisplaySettingsChanged += OnDisplaySettingsChanged;
            SystemEvents.PowerModeChanged += OnPowerModeChanged;
            SystemEvents.SessionEnding += OnSessionEnding;

            controller.Start();
        }

        internal void EmergencyRestore()
        {
            try
            {
                controller.RequestStopAndRestore();
            }
            catch (Exception exception)
            {
                AppLog.Write("Emergency restore failed: " + exception);
            }
        }

        private void OnModeChanged(DimmerMode mode)
        {
            dailyModeItem.Checked = mode == DimmerMode.Daily;
            movieModeItem.Checked = mode == DimmerMode.Movie;
            customModeItem.Checked = mode == DimmerMode.Custom;
            offModeItem.Checked = mode == DimmerMode.Off;

            if (settingsForm != null && !settingsForm.IsDisposed)
            {
                settingsForm.SyncMode(mode);
            }
        }

        internal void ShowSettingsForm()
        {
            if (settingsForm == null || settingsForm.IsDisposed)
            {
                settingsForm = new SettingsForm(controller, appIcon);
                settingsForm.FormClosed += delegate { settingsForm = null; };
                settingsForm.Show();
            }
            else
            {
                settingsForm.RefreshFromController();
                if (settingsForm.WindowState == FormWindowState.Minimized)
                {
                    settingsForm.WindowState = FormWindowState.Normal;
                }

                settingsForm.Show();
                settingsForm.Activate();
                settingsForm.BringToFront();
            }
        }

        private void OnStartupCheckedChanged(object sender, EventArgs eventArgs)
        {
            if (changingStartupItem)
            {
                return;
            }

            OperationResult result = StartupManager.SetEnabled(startupItem.Checked);
            OnStatusChanged(result.Message);

            if (!result.Success)
            {
                changingStartupItem = true;
                startupItem.Checked = !startupItem.Checked;
                changingStartupItem = false;
            }
        }

        private void OnStatusChanged(string status)
        {
            statusItem.Text = status;
            trayIcon.Text = status.Length > 60 ? status.Substring(0, 60) : status;
        }

        private void OnDisplaySettingsChanged(object sender, EventArgs eventArgs)
        {
            controller.RequestRefreshTarget();

            if (settingsForm != null &&
                !settingsForm.IsDisposed &&
                settingsForm.IsHandleCreated)
            {
                try
                {
                    settingsForm.BeginInvoke(new Action(settingsForm.RefreshFromController));
                }
                catch
                {
                    // The settings form may be closing while displays are changing.
                }
            }
        }

        private void OnPowerModeChanged(object sender, PowerModeChangedEventArgs eventArgs)
        {
            if (eventArgs.Mode == PowerModes.Suspend)
            {
                controller.RequestForceRestore();
            }
            else if (eventArgs.Mode == PowerModes.Resume)
            {
                controller.RequestRefreshTarget();
            }
        }

        private void OnSessionEnding(object sender, SessionEndingEventArgs eventArgs)
        {
            controller.RequestStopAndRestore();
        }

        private void ExitApplication()
        {
            if (exiting)
            {
                return;
            }

            exiting = true;
            ExitThread();
        }

        protected override void ExitThreadCore()
        {
            SystemEvents.DisplaySettingsChanged -= OnDisplaySettingsChanged;
            SystemEvents.PowerModeChanged -= OnPowerModeChanged;
            SystemEvents.SessionEnding -= OnSessionEnding;

            controller.StatusChanged -= OnStatusChanged;
            controller.ModeChanged -= OnModeChanged;

            if (settingsForm != null && !settingsForm.IsDisposed)
            {
                SettingsForm form = settingsForm;
                settingsForm = null;
                form.Close();
                form.Dispose();
            }

            controller.Dispose();

            trayIcon.Visible = false;
            trayIcon.Dispose();

            if (appIcon != null)
            {
                appIcon.Dispose();
            }

            base.ExitThreadCore();
        }

        private static Icon LoadApplicationIcon()
        {
            try
            {
                return Icon.ExtractAssociatedIcon(Application.ExecutablePath);
            }
            catch (Exception exception)
            {
                AppLog.Write("Loading application icon failed: " + exception.Message);
                return null;
            }
        }
    }
}
