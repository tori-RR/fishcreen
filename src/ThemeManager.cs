using System;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Windows.Forms;
using Microsoft.Win32;

namespace SecondScreenDimmer
{
    internal static class ThemeManager
    {
        internal const string AccentRole = "Accent";
        internal const string SecondaryTextRole = "SecondaryText";
        internal const string PrimaryButtonRole = "PrimaryButton";
        internal const string SecondaryButtonRole = "SecondaryButton";

        private const int ImmersiveDarkModeAttribute = 20;
        private const int LegacyImmersiveDarkModeAttribute = 19;
        private const int BorderColorAttribute = 34;
        private const int CaptionColorAttribute = 35;
        private const int CaptionTextColorAttribute = 36;

        [DllImport("dwmapi.dll")]
        private static extern int DwmSetWindowAttribute(
            IntPtr windowHandle,
            int attribute,
            ref int value,
            int valueSize);

        [DllImport("uxtheme.dll", CharSet = CharSet.Unicode)]
        private static extern int SetWindowTheme(
            IntPtr windowHandle,
            string subApplicationName,
            string subIdentifierList);

        internal static bool IsDark(AppThemeMode mode)
        {
            if (mode == AppThemeMode.Dark)
            {
                return true;
            }

            if (mode == AppThemeMode.Light)
            {
                return false;
            }

            try
            {
                using (RegistryKey key = Registry.CurrentUser.OpenSubKey(
                    @"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"))
                {
                    object value = key == null ? null : key.GetValue("AppsUseLightTheme");
                    if (value is int)
                    {
                        return (int)value == 0;
                    }
                }
            }
            catch
            {
                // Use the light theme if Windows does not expose the preference.
            }

            return false;
        }

        internal static void ApplyFormTheme(Form form, AppThemeMode mode)
        {
            bool dark = IsDark(mode);
            Color background = dark
                ? Color.FromArgb(14, 14, 14)
                : Color.FromArgb(245, 246, 248);

            form.BackColor = background;
            form.ForeColor = dark
                ? Color.FromArgb(222, 222, 222)
                : Color.FromArgb(32, 35, 40);

            ApplyControlCollection(form.Controls, dark, background);
            ApplyWindowFrame(form, dark);
            form.Invalidate(true);
        }

        internal static void ApplyContextMenuTheme(
            ContextMenuStrip menu,
            AppThemeMode mode)
        {
            bool dark = IsDark(mode);
            Color background = dark
                ? Color.FromArgb(37, 40, 46)
                : Color.FromArgb(250, 250, 250);
            Color foreground = dark
                ? Color.FromArgb(238, 240, 244)
                : Color.FromArgb(32, 35, 40);

            menu.BackColor = background;
            menu.ForeColor = foreground;
            menu.Renderer = new ToolStripProfessionalRenderer(new ThemeColorTable(dark));
            ApplyMenuItems(menu.Items, background, foreground);
        }

        private static void ApplyControlCollection(
            Control.ControlCollection controls,
            bool dark,
            Color background)
        {
            foreach (Control control in controls)
            {
                Color childBackground = background;
                RoundedPanel roundedPanel = control as RoundedPanel;
                if (roundedPanel != null)
                {
                    roundedPanel.DarkMode = dark;
                    roundedPanel.BackColor = roundedPanel.SurfaceColor;
                    childBackground = roundedPanel.SurfaceColor;
                }
                else
                {
                    Panel panel = control as Panel;
                    if (panel != null)
                    {
                        panel.BackColor = background;
                        ApplyNativeControlTheme(panel, dark);
                    }
                }

                Label label = control as Label;
                if (label != null)
                {
                    string role = label.Tag as string;
                    if (string.Equals(role, AccentRole, StringComparison.Ordinal))
                    {
                        label.ForeColor = Color.FromArgb(45, 139, 255);
                    }
                    else if (string.Equals(
                        role,
                        SecondaryTextRole,
                        StringComparison.Ordinal))
                    {
                        label.ForeColor = dark
                            ? Color.FromArgb(137, 137, 137)
                            : Color.FromArgb(92, 99, 112);
                    }
                    else
                    {
                        label.ForeColor = dark
                            ? Color.FromArgb(222, 222, 222)
                            : Color.FromArgb(32, 35, 40);
                    }

                    label.BackColor = background;
                }

                ComboBox comboBox = control as ComboBox;
                if (comboBox != null)
                {
                    comboBox.BackColor = dark
                        ? Color.FromArgb(16, 16, 16)
                        : Color.FromArgb(247, 248, 250);
                    comboBox.ForeColor = dark
                        ? Color.FromArgb(222, 222, 222)
                        : Color.FromArgb(32, 35, 40);
                    comboBox.FlatStyle = FlatStyle.Flat;
                    ApplyNativeControlTheme(comboBox, dark);
                }

                TrackBar trackBar = control as TrackBar;
                if (trackBar != null)
                {
                    trackBar.BackColor = background;
                    trackBar.ForeColor = dark
                        ? Color.FromArgb(238, 240, 244)
                        : Color.FromArgb(32, 35, 40);
                    ApplyNativeControlTheme(trackBar, dark);
                }

                ModernSlider modernSlider = control as ModernSlider;
                if (modernSlider != null)
                {
                    modernSlider.DarkMode = dark;
                    modernSlider.Invalidate();
                }

                Button button = control as Button;
                if (button != null)
                {
                    string role = button.Tag as string;
                    button.FlatStyle = FlatStyle.Flat;

                    if (string.Equals(
                        role,
                        PrimaryButtonRole,
                        StringComparison.Ordinal))
                    {
                        button.BackColor = Color.FromArgb(35, 122, 230);
                        button.ForeColor = Color.White;
                        button.FlatAppearance.BorderSize = 0;
                    }
                    else
                    {
                        button.BackColor = dark
                            ? Color.FromArgb(38, 38, 38)
                            : Color.FromArgb(244, 245, 247);
                        button.ForeColor = dark
                            ? Color.FromArgb(222, 222, 222)
                            : Color.FromArgb(32, 35, 40);
                        button.FlatAppearance.BorderColor = dark
                            ? Color.FromArgb(58, 58, 58)
                            : Color.FromArgb(190, 194, 201);
                        button.FlatAppearance.BorderSize = 1;
                    }
                }

                ToggleSwitch toggleSwitch = control as ToggleSwitch;
                if (toggleSwitch != null)
                {
                    toggleSwitch.DarkMode = dark;
                    toggleSwitch.Invalidate();
                }

                if (control.HasChildren)
                {
                    ApplyControlCollection(control.Controls, dark, childBackground);
                }
            }
        }

        private static void ApplyNativeControlTheme(Control control, bool dark)
        {
            try
            {
                SetWindowTheme(
                    control.Handle,
                    dark ? "DarkMode_Explorer" : "Explorer",
                    null);
            }
            catch
            {
                // Manual colors still provide a usable fallback.
            }
        }

        private static void ApplyWindowFrame(Form form, bool dark)
        {
            try
            {
                int enabled = dark ? 1 : 0;
                int result = DwmSetWindowAttribute(
                    form.Handle,
                    ImmersiveDarkModeAttribute,
                    ref enabled,
                    sizeof(int));

                if (result != 0)
                {
                    DwmSetWindowAttribute(
                        form.Handle,
                        LegacyImmersiveDarkModeAttribute,
                        ref enabled,
                        sizeof(int));
                }

                int captionColor = ToColorReference(
                    dark
                        ? Color.FromArgb(14, 14, 14)
                        : Color.FromArgb(245, 246, 248));
                int captionTextColor = ToColorReference(
                    dark
                        ? Color.FromArgb(222, 222, 222)
                        : Color.FromArgb(32, 35, 40));
                int borderColor = ToColorReference(
                    dark
                        ? Color.FromArgb(76, 81, 91)
                        : Color.FromArgb(205, 208, 214));

                DwmSetWindowAttribute(
                    form.Handle,
                    CaptionColorAttribute,
                    ref captionColor,
                    sizeof(int));
                DwmSetWindowAttribute(
                    form.Handle,
                    CaptionTextColorAttribute,
                    ref captionTextColor,
                    sizeof(int));
                DwmSetWindowAttribute(
                    form.Handle,
                    BorderColorAttribute,
                    ref borderColor,
                    sizeof(int));
            }
            catch
            {
                // Older Windows versions may not support dark title bars.
            }
        }

        private static int ToColorReference(Color color)
        {
            return color.R | (color.G << 8) | (color.B << 16);
        }

        private static void ApplyMenuItems(
            ToolStripItemCollection items,
            Color background,
            Color foreground)
        {
            foreach (ToolStripItem item in items)
            {
                item.BackColor = background;
                item.ForeColor = foreground;

                ToolStripMenuItem menuItem = item as ToolStripMenuItem;
                if (menuItem != null && menuItem.HasDropDownItems)
                {
                    menuItem.DropDown.BackColor = background;
                    menuItem.DropDown.ForeColor = foreground;
                    menuItem.DropDown.Renderer = menuItem.Owner == null
                        ? null
                        : menuItem.Owner.Renderer;
                    ApplyMenuItems(menuItem.DropDownItems, background, foreground);
                }
            }
        }

        private sealed class ThemeColorTable : ProfessionalColorTable
        {
            private readonly bool dark;

            internal ThemeColorTable(bool dark)
            {
                this.dark = dark;
                UseSystemColors = false;
            }

            private Color Background
            {
                get { return dark ? Color.FromArgb(37, 40, 46) : Color.White; }
            }

            private Color Selected
            {
                get
                {
                    return dark
                        ? Color.FromArgb(59, 64, 74)
                        : Color.FromArgb(224, 233, 246);
                }
            }

            public override Color ToolStripDropDownBackground
            {
                get { return Background; }
            }

            public override Color ImageMarginGradientBegin
            {
                get { return Background; }
            }

            public override Color ImageMarginGradientMiddle
            {
                get { return Background; }
            }

            public override Color ImageMarginGradientEnd
            {
                get { return Background; }
            }

            public override Color MenuItemSelected
            {
                get { return Selected; }
            }

            public override Color MenuItemBorder
            {
                get { return Selected; }
            }

            public override Color MenuItemSelectedGradientBegin
            {
                get { return Selected; }
            }

            public override Color MenuItemSelectedGradientEnd
            {
                get { return Selected; }
            }

            public override Color MenuItemPressedGradientBegin
            {
                get { return Selected; }
            }

            public override Color MenuItemPressedGradientEnd
            {
                get { return Selected; }
            }

            public override Color SeparatorDark
            {
                get
                {
                    return dark
                        ? Color.FromArgb(76, 81, 91)
                        : Color.FromArgb(205, 208, 214);
                }
            }

            public override Color SeparatorLight
            {
                get { return SeparatorDark; }
            }

            public override Color ToolStripBorder
            {
                get { return SeparatorDark; }
            }
        }
    }
}
