using System;
using System.Collections.Generic;
using System.Drawing;
using System.Globalization;
using System.Windows.Forms;
using Microsoft.Win32;

namespace SecondScreenDimmer
{
    internal sealed class SettingsForm : Form
    {
        private readonly ScreenDimmerController controller;
        private readonly ComboBox monitorComboBox;
        private readonly Label monitorDetailLabel;
        private readonly ComboBox modeComboBox;
        private readonly ComboBox themeComboBox;
        private readonly ModernSlider leaveDelayTrackBar;
        private readonly Label leaveDelayValueLabel;
        private readonly ModernSlider idleBlackoutTrackBar;
        private readonly Label idleBlackoutValueLabel;
        private readonly ModernSlider blackoutRecoveryTrackBar;
        private readonly Label blackoutRecoveryValueLabel;
        private readonly ModernSlider brightnessStepsTrackBar;
        private readonly Label brightnessStepsValueLabel;
        private readonly ModernSlider brightnessExponentTrackBar;
        private readonly Label brightnessExponentValueLabel;
        private readonly ToggleSwitch brightnessFadeSwitch;
        private readonly ToggleSwitch blackoutFadeSwitch;
        private bool loadingProfile;

        internal SettingsForm(ScreenDimmerController controller, Icon icon)
        {
            this.controller = controller;

            AutoScaleDimensions = new SizeF(96F, 96F);
            AutoScaleMode = AutoScaleMode.Dpi;
            BackColor = Color.FromArgb(247, 248, 250);
            ClientSize = new Size(680, 780);
            Font = new Font("Segoe UI", 9F, FontStyle.Regular, GraphicsUnit.Point);
            FormBorderStyle = FormBorderStyle.FixedSingle;
            MaximizeBox = false;
            MinimizeBox = false;
            ShowInTaskbar = true;
            StartPosition = FormStartPosition.CenterScreen;
            Text = "fishcreen 设置";

            if (icon != null)
            {
                Icon = (Icon)icon.Clone();
            }

            Label titleLabel = new Label();
            titleLabel.AutoSize = true;
            titleLabel.Font = new Font("Segoe UI", 16F, FontStyle.Bold, GraphicsUnit.Point);
            titleLabel.Location = new Point(24, 20);
            titleLabel.Tag = "PrimaryText";
            titleLabel.Text = "fishcreen";

            Label subtitleLabel = new Label();
            subtitleLabel.AutoSize = true;
            subtitleLabel.ForeColor = Color.FromArgb(92, 99, 112);
            subtitleLabel.Location = new Point(27, 54);
            subtitleLabel.Tag = ThemeManager.SecondaryTextRole;
            subtitleLabel.Text = "副屏亮度与黑屏行为";

            monitorComboBox = new ComboBox();
            monitorComboBox.DropDownStyle = ComboBoxStyle.DropDownList;
            monitorComboBox.Location = new Point(24, 111);
            monitorComboBox.Size = new Size(512, 24);
            monitorComboBox.SelectedIndexChanged += OnMonitorSelectionChanged;

            monitorDetailLabel = new Label();
            monitorDetailLabel.AutoEllipsis = true;
            monitorDetailLabel.ForeColor = Color.FromArgb(104, 111, 124);
            monitorDetailLabel.Location = new Point(27, 140);
            monitorDetailLabel.Size = new Size(506, 18);
            monitorDetailLabel.Tag = ThemeManager.SecondaryTextRole;

            modeComboBox = new ComboBox();
            modeComboBox.DropDownStyle = ComboBoxStyle.DropDownList;
            modeComboBox.Location = new Point(24, 194);
            modeComboBox.Size = new Size(248, 24);
            modeComboBox.Items.Add(new ModeOption(DimmerMode.Daily, "日常模式（预设）"));
            modeComboBox.Items.Add(new ModeOption(DimmerMode.Movie, "观影模式（预设）"));
            modeComboBox.Items.Add(new ModeOption(DimmerMode.Custom, "自定义"));
            modeComboBox.SelectedIndexChanged += OnModeSelectionChanged;

            themeComboBox = new ComboBox();
            themeComboBox.DropDownStyle = ComboBoxStyle.DropDownList;
            themeComboBox.Location = new Point(288, 194);
            themeComboBox.Size = new Size(248, 24);
            themeComboBox.Items.Add(new ThemeOption(AppThemeMode.System, "跟随系统"));
            themeComboBox.Items.Add(new ThemeOption(AppThemeMode.Light, "明亮"));
            themeComboBox.Items.Add(new ThemeOption(AppThemeMode.Dark, "黑暗"));
            themeComboBox.SelectedIndexChanged += OnThemeSelectionChanged;

            leaveDelayValueLabel = CreateValueLabel(462, 234);
            leaveDelayTrackBar = new ModernSlider();
            leaveDelayTrackBar.AutoSize = false;
            leaveDelayTrackBar.Location = new Point(19, 257);
            leaveDelayTrackBar.Maximum = 100;
            leaveDelayTrackBar.Minimum = 5;
            leaveDelayTrackBar.SmallChange = 5;
            leaveDelayTrackBar.LargeChange = 10;
            leaveDelayTrackBar.TickFrequency = 10;
            leaveDelayTrackBar.Size = new Size(522, 34);
            leaveDelayTrackBar.ValueChanged += OnProfileParameterChanged;

            idleBlackoutValueLabel = CreateValueLabel(462, 300);
            idleBlackoutTrackBar = new ModernSlider();
            idleBlackoutTrackBar.AutoSize = false;
            idleBlackoutTrackBar.Location = new Point(19, 323);
            idleBlackoutTrackBar.Maximum = 300;
            idleBlackoutTrackBar.Minimum = 5;
            idleBlackoutTrackBar.SmallChange = 5;
            idleBlackoutTrackBar.LargeChange = 30;
            idleBlackoutTrackBar.TickFrequency = 30;
            idleBlackoutTrackBar.Size = new Size(522, 34);
            idleBlackoutTrackBar.ValueChanged += OnProfileParameterChanged;

            blackoutRecoveryValueLabel = CreateValueLabel(462, 366);
            blackoutRecoveryTrackBar = new ModernSlider();
            blackoutRecoveryTrackBar.AutoSize = false;
            blackoutRecoveryTrackBar.Location = new Point(19, 389);
            blackoutRecoveryTrackBar.Maximum = 50;
            blackoutRecoveryTrackBar.Minimum = 1;
            blackoutRecoveryTrackBar.SmallChange = 1;
            blackoutRecoveryTrackBar.LargeChange = 5;
            blackoutRecoveryTrackBar.TickFrequency = 5;
            blackoutRecoveryTrackBar.Size = new Size(522, 34);
            blackoutRecoveryTrackBar.ValueChanged += OnProfileParameterChanged;

            brightnessStepsValueLabel = CreateValueLabel(462, 366);
            brightnessStepsTrackBar = new ModernSlider();
            brightnessStepsTrackBar.AutoSize = false;
            brightnessStepsTrackBar.Location = new Point(19, 389);
            brightnessStepsTrackBar.Maximum = DimmerProfile.MaximumBrightnessFadeSteps;
            brightnessStepsTrackBar.Minimum = DimmerProfile.MinimumBrightnessFadeSteps;
            brightnessStepsTrackBar.SmallChange = 1;
            brightnessStepsTrackBar.LargeChange = 5;
            brightnessStepsTrackBar.TickFrequency = 5;
            brightnessStepsTrackBar.Size = new Size(522, 34);
            brightnessStepsTrackBar.ValueChanged += OnProfileParameterChanged;

            brightnessExponentValueLabel = CreateValueLabel(462, 432);
            brightnessExponentTrackBar = new ModernSlider();
            brightnessExponentTrackBar.AutoSize = false;
            brightnessExponentTrackBar.Location = new Point(19, 455);
            brightnessExponentTrackBar.Maximum = 40;
            brightnessExponentTrackBar.Minimum = 10;
            brightnessExponentTrackBar.SmallChange = 1;
            brightnessExponentTrackBar.LargeChange = 5;
            brightnessExponentTrackBar.TickFrequency = 5;
            brightnessExponentTrackBar.Size = new Size(522, 34);
            brightnessExponentTrackBar.ValueChanged += OnProfileParameterChanged;

            brightnessFadeSwitch = new ToggleSwitch();
            brightnessFadeSwitch.Location = new Point(486, 502);
            brightnessFadeSwitch.CheckedChanged += OnProfileParameterChanged;

            blackoutFadeSwitch = new ToggleSwitch();
            blackoutFadeSwitch.Location = new Point(486, 546);
            blackoutFadeSwitch.CheckedChanged += OnProfileParameterChanged;

            Button cancelButton = new Button();
            cancelButton.DialogResult = DialogResult.Cancel;
            cancelButton.Location = new Point(396, 730);
            cancelButton.Size = new Size(80, 30);
            cancelButton.Tag = ThemeManager.SecondaryButtonRole;
            cancelButton.Text = "取消";

            Button applyButton = new Button();
            applyButton.Location = new Point(486, 730);
            applyButton.Size = new Size(80, 30);
            applyButton.Tag = ThemeManager.SecondaryButtonRole;
            applyButton.Text = "应用";
            applyButton.Click += OnApplyClicked;

            Button saveButton = new Button();
            saveButton.BackColor = Color.FromArgb(35, 122, 230);
            saveButton.FlatAppearance.BorderSize = 0;
            saveButton.FlatStyle = FlatStyle.Flat;
            saveButton.ForeColor = Color.White;
            saveButton.Location = new Point(576, 730);
            saveButton.Size = new Size(80, 30);
            saveButton.Tag = ThemeManager.PrimaryButtonRole;
            saveButton.Text = "保存";
            saveButton.Click += OnSaveClicked;

            AcceptButton = saveButton;
            CancelButton = cancelButton;

            FlowLayoutPanel contentPanel = new FlowLayoutPanel();
            contentPanel.AutoScroll = true;
            contentPanel.FlowDirection = FlowDirection.TopDown;
            contentPanel.Location = new Point(24, 82);
            contentPanel.Padding = new Padding(0, 0, 10, 18);
            contentPanel.Size = new Size(632, 624);
            contentPanel.WrapContents = false;

            contentPanel.Controls.Add(CreateGroupHeading("基础"));

            RoundedPanel monitorCard = CreateSettingCard(
                "生效屏幕",
                "选择由 fishcreen 控制亮度和黑屏的显示器",
                78);
            ReplaceCardDescription(monitorCard, monitorDetailLabel);
            PrepareComboBox(monitorComboBox, "生效屏幕");
            monitorCard.Controls.Add(monitorComboBox);
            contentPanel.Controls.Add(monitorCard);

            RoundedPanel modeCard = CreateSettingCard(
                "运行模式",
                "使用固定预设，或切换到可调整参数的自定义模式",
                78);
            PrepareComboBox(modeComboBox, "运行模式");
            modeCard.Controls.Add(modeComboBox);
            contentPanel.Controls.Add(modeCard);

            RoundedPanel themeCard = CreateSettingCard(
                "界面外观",
                "跟随 Windows 系统，也可以强制使用明亮或黑暗主题",
                78);
            PrepareComboBox(themeComboBox, "界面外观");
            themeCard.Controls.Add(themeComboBox);
            contentPanel.Controls.Add(themeCard);

            contentPanel.Controls.Add(CreateGroupHeading("自动暗屏"));
            contentPanel.Controls.Add(CreateSliderCard(
                "鼠标离开后变暗",
                "指针离开所选屏幕后，等待多久开始降低硬件亮度",
                leaveDelayTrackBar,
                leaveDelayValueLabel));
            contentPanel.Controls.Add(CreateSliderCard(
                "鼠标无动作后黑屏",
                "持续无鼠标动作达到此时间后显示黑屏遮罩",
                idleBlackoutTrackBar,
                idleBlackoutValueLabel));

            contentPanel.Controls.Add(CreateGroupHeading("渐变效果"));
            contentPanel.Controls.Add(CreateSwitchCard(
                "亮度渐变",
                "降低硬件亮度时使用前快后慢曲线",
                brightnessFadeSwitch));
            contentPanel.Controls.Add(CreateSliderCard(
                "亮度曲线等级",
                "等级越高，暗场阶段的亮度变化越细腻",
                brightnessStepsTrackBar,
                brightnessStepsValueLabel));
            contentPanel.Controls.Add(CreateSliderCard(
                "曲线指数",
                "数值越高，前段变暗越快、后段过渡越慢",
                brightnessExponentTrackBar,
                brightnessExponentValueLabel));
            contentPanel.Controls.Add(CreateSwitchCard(
                "黑屏渐变",
                "日常或自定义模式进入黑屏时使用 5 秒渐变",
                blackoutFadeSwitch));
            contentPanel.Controls.Add(CreateSliderCard(
                "黑幕恢复时间",
                "鼠标活动后，从当前黑幕平滑恢复到完全无遮罩",
                blackoutRecoveryTrackBar,
                blackoutRecoveryValueLabel));

            Controls.Add(titleLabel);
            Controls.Add(subtitleLabel);
            Controls.Add(contentPanel);
            Controls.Add(cancelButton);
            Controls.Add(applyButton);
            Controls.Add(saveButton);

            SystemEvents.UserPreferenceChanged += OnSystemUserPreferenceChanged;
            RefreshFromController();
        }

        internal void RefreshFromController()
        {
            AppSettings settings = controller.GetSettingsSnapshot();
            loadingProfile = true;
            PopulateMonitors(settings.TargetStableId);

            DimmerMode selectedMode = settings.Mode;
            if (selectedMode != DimmerMode.Movie && selectedMode != DimmerMode.Custom)
            {
                selectedMode = DimmerMode.Daily;
            }

            SelectMode(selectedMode);
            SelectTheme(settings.ThemeMode);
            LoadProfile(GetProfileForMode(settings, selectedMode));
            loadingProfile = false;
            UpdateValueLabels();
            ApplySelectedTheme();
        }

        internal void SyncMode(DimmerMode mode)
        {
            if (mode == DimmerMode.Daily ||
                mode == DimmerMode.Movie ||
                mode == DimmerMode.Custom)
            {
                loadingProfile = true;
                SelectMode(mode);
                LoadProfile(GetProfileForMode(controller.GetSettingsSnapshot(), mode));
                loadingProfile = false;
                UpdateValueLabels();
            }
        }

        private void PopulateMonitors(string selectedStableId)
        {
            monitorComboBox.BeginUpdate();
            monitorComboBox.Items.Clear();

            List<MonitorDescriptor> monitors = controller.GetAvailableMonitors();
            int selectedIndex = -1;

            foreach (MonitorDescriptor monitor in monitors)
            {
                MonitorOption option = new MonitorOption(monitor);
                int index = monitorComboBox.Items.Add(option);
                if (string.Equals(
                    option.StableId,
                    selectedStableId,
                    StringComparison.OrdinalIgnoreCase))
                {
                    selectedIndex = index;
                }
            }

            if (selectedIndex < 0 && !string.IsNullOrEmpty(selectedStableId))
            {
                selectedIndex = monitorComboBox.Items.Add(
                    new MonitorOption(selectedStableId, "已选择的屏幕（当前未连接）"));
            }

            if (selectedIndex < 0 && monitorComboBox.Items.Count > 0)
            {
                selectedIndex = 0;
            }

            monitorComboBox.SelectedIndex = selectedIndex;
            monitorComboBox.EndUpdate();
            OnMonitorSelectionChanged(null, EventArgs.Empty);
        }

        private void SelectMode(DimmerMode mode)
        {
            foreach (object item in modeComboBox.Items)
            {
                ModeOption option = item as ModeOption;
                if (option != null && option.Mode == mode)
                {
                    modeComboBox.SelectedItem = option;
                    return;
                }
            }

            modeComboBox.SelectedIndex = 0;
        }

        private void SelectTheme(AppThemeMode mode)
        {
            foreach (object item in themeComboBox.Items)
            {
                ThemeOption option = item as ThemeOption;
                if (option != null && option.Mode == mode)
                {
                    themeComboBox.SelectedItem = option;
                    return;
                }
            }

            themeComboBox.SelectedIndex = 0;
        }

        private void OnMonitorSelectionChanged(object sender, EventArgs eventArgs)
        {
            MonitorOption option = monitorComboBox.SelectedItem as MonitorOption;
            monitorDetailLabel.Text = option == null ? "未检测到可用屏幕" : option.Detail;
        }

        private void OnModeSelectionChanged(object sender, EventArgs eventArgs)
        {
            if (loadingProfile)
            {
                return;
            }

            ModeOption selectedMode = modeComboBox.SelectedItem as ModeOption;
            if (selectedMode == null)
            {
                return;
            }

            loadingProfile = true;
            LoadProfile(GetProfileForMode(controller.GetSettingsSnapshot(), selectedMode.Mode));
            loadingProfile = false;
            UpdateValueLabels();
        }

        private void OnThemeSelectionChanged(object sender, EventArgs eventArgs)
        {
            ApplySelectedTheme();
        }

        private void OnProfileParameterChanged(object sender, EventArgs eventArgs)
        {
            UpdateValueLabels();

            if (loadingProfile)
            {
                return;
            }

            ModeOption selectedMode = modeComboBox.SelectedItem as ModeOption;
            if (selectedMode != null && selectedMode.Mode != DimmerMode.Custom)
            {
                loadingProfile = true;
                SelectMode(DimmerMode.Custom);
                loadingProfile = false;
            }
        }

        private void LoadProfile(DimmerProfile profile)
        {
            leaveDelayTrackBar.Value = Math.Max(
                leaveDelayTrackBar.Minimum,
                Math.Min(leaveDelayTrackBar.Maximum, profile.LeaveDelayMilliseconds / 100));

            idleBlackoutTrackBar.Value = Math.Max(
                idleBlackoutTrackBar.Minimum,
                Math.Min(idleBlackoutTrackBar.Maximum, profile.IdleBlackoutMilliseconds / 1000));

            blackoutRecoveryTrackBar.Value = Math.Max(
                blackoutRecoveryTrackBar.Minimum,
                Math.Min(
                    blackoutRecoveryTrackBar.Maximum,
                    profile.BlackoutRecoveryMilliseconds / 100));

            brightnessStepsTrackBar.Value = Math.Max(
                brightnessStepsTrackBar.Minimum,
                Math.Min(brightnessStepsTrackBar.Maximum, profile.BrightnessFadeSteps));

            brightnessExponentTrackBar.Value = Math.Max(
                brightnessExponentTrackBar.Minimum,
                Math.Min(
                    brightnessExponentTrackBar.Maximum,
                    (int)Math.Round(profile.BrightnessFadeExponent * 10.0)));

            brightnessFadeSwitch.Checked = profile.BrightnessFadeEnabled;
            blackoutFadeSwitch.Checked = profile.BlackoutFadeEnabled;
        }

        private static DimmerProfile GetProfileForMode(AppSettings settings, DimmerMode mode)
        {
            switch (mode)
            {
                case DimmerMode.Movie:
                    return AppSettings.CreateMovieProfile();
                case DimmerMode.Custom:
                    return settings.CustomProfile.Clone();
                default:
                    return AppSettings.CreateDailyProfile();
            }
        }

        private void UpdateValueLabels()
        {
            leaveDelayValueLabel.Text = (leaveDelayTrackBar.Value / 10.0).ToString(
                "0.0",
                CultureInfo.CurrentCulture) + " 秒";
            idleBlackoutValueLabel.Text = idleBlackoutTrackBar.Value + " 秒";
            blackoutRecoveryValueLabel.Text =
                (blackoutRecoveryTrackBar.Value / 10.0).ToString(
                "0.0",
                CultureInfo.CurrentCulture) + " 秒";
            brightnessStepsValueLabel.Text = brightnessStepsTrackBar.Value + " 段";
            brightnessExponentValueLabel.Text = (brightnessExponentTrackBar.Value / 10.0).ToString(
                "0.0",
                CultureInfo.CurrentCulture);
            brightnessStepsTrackBar.Enabled = brightnessFadeSwitch.Checked;
            brightnessExponentTrackBar.Enabled = brightnessFadeSwitch.Checked;
        }

        private void OnApplyClicked(object sender, EventArgs eventArgs)
        {
            ApplyCurrentSettings();
        }

        private void OnSaveClicked(object sender, EventArgs eventArgs)
        {
            if (!ApplyCurrentSettings())
            {
                return;
            }

            DialogResult = DialogResult.OK;
            Close();
        }

        private bool ApplyCurrentSettings()
        {
            MonitorOption monitor = monitorComboBox.SelectedItem as MonitorOption;
            ModeOption selectedMode = modeComboBox.SelectedItem as ModeOption;

            if (monitor == null || string.IsNullOrEmpty(monitor.StableId))
            {
                MessageBox.Show(
                    this,
                    "请先选择一个生效屏幕。",
                    "fishcreen",
                    MessageBoxButtons.OK,
                    MessageBoxIcon.Information);
                return false;
            }

            AppSettings settings = controller.GetSettingsSnapshot();
            settings.TargetStableId = monitor.StableId;
            settings.Mode = selectedMode == null ? DimmerMode.Daily : selectedMode.Mode;
            settings.ThemeMode = GetSelectedThemeMode();

            if (settings.Mode == DimmerMode.Custom)
            {
                settings.CustomProfile.LeaveDelayMilliseconds = leaveDelayTrackBar.Value * 100;
                settings.CustomProfile.IdleBlackoutMilliseconds = idleBlackoutTrackBar.Value * 1000;
                settings.CustomProfile.BlackoutRecoveryMilliseconds =
                    blackoutRecoveryTrackBar.Value * 100;
                settings.CustomProfile.BrightnessFadeEnabled = brightnessFadeSwitch.Checked;
                settings.CustomProfile.BlackoutFadeEnabled = blackoutFadeSwitch.Checked;
                settings.CustomProfile.BrightnessFadeSteps = brightnessStepsTrackBar.Value;
                settings.CustomProfile.BrightnessFadeExponent =
                    brightnessExponentTrackBar.Value / 10.0;
            }

            controller.ApplySettings(settings);
            return true;
        }

        private AppThemeMode GetSelectedThemeMode()
        {
            ThemeOption option = themeComboBox.SelectedItem as ThemeOption;
            return option == null ? AppThemeMode.System : option.Mode;
        }

        private void ApplySelectedTheme()
        {
            if (themeComboBox == null)
            {
                return;
            }

            ThemeManager.ApplyFormTheme(this, GetSelectedThemeMode());
        }

        private void OnSystemUserPreferenceChanged(
            object sender,
            UserPreferenceChangedEventArgs eventArgs)
        {
            if (GetSelectedThemeMode() != AppThemeMode.System ||
                IsDisposed ||
                !IsHandleCreated)
            {
                return;
            }

            try
            {
                BeginInvoke(new Action(ApplySelectedTheme));
            }
            catch
            {
                // The form may be closing while Windows changes its theme.
            }
        }

        protected override void OnShown(EventArgs eventArgs)
        {
            base.OnShown(eventArgs);
            ApplySelectedTheme();
        }

        protected override void OnFormClosed(FormClosedEventArgs eventArgs)
        {
            SystemEvents.UserPreferenceChanged -= OnSystemUserPreferenceChanged;
            base.OnFormClosed(eventArgs);
        }

        private static Label CreateGroupHeading(string text)
        {
            Label label = new Label();
            label.AutoSize = false;
            label.Font = new Font("Segoe UI", 11F, FontStyle.Bold, GraphicsUnit.Point);
            label.Margin = new Padding(12, 10, 0, 4);
            label.Size = new Size(584, 30);
            label.Tag = "PrimaryText";
            label.Text = text;
            label.TextAlign = ContentAlignment.MiddleLeft;
            return label;
        }

        private static RoundedPanel CreateSettingCard(
            string title,
            string description,
            int height)
        {
            RoundedPanel card = new RoundedPanel();
            card.CornerRadius = 16;
            card.Margin = new Padding(0, 0, 0, 10);
            card.Size = new Size(596, height);

            Label titleLabel = new Label();
            titleLabel.AutoSize = false;
            titleLabel.Font = new Font(
                "Segoe UI",
                10F,
                FontStyle.Regular,
                GraphicsUnit.Point);
            titleLabel.Location = new Point(18, 13);
            titleLabel.Size = new Size(306, 23);
            titleLabel.Tag = "PrimaryText";
            titleLabel.Text = title;

            Label descriptionLabel = new Label();
            descriptionLabel.AutoEllipsis = true;
            descriptionLabel.Font = new Font(
                "Segoe UI",
                8.5F,
                FontStyle.Regular,
                GraphicsUnit.Point);
            descriptionLabel.Location = new Point(18, 40);
            descriptionLabel.Name = "CardDescription";
            descriptionLabel.Size = new Size(310, 20);
            descriptionLabel.Tag = ThemeManager.SecondaryTextRole;
            descriptionLabel.Text = description;

            card.Controls.Add(titleLabel);
            card.Controls.Add(descriptionLabel);
            return card;
        }

        private static void ReplaceCardDescription(
            RoundedPanel card,
            Label descriptionLabel)
        {
            Control previous = card.Controls["CardDescription"];
            if (previous != null)
            {
                card.Controls.Remove(previous);
                previous.Dispose();
            }

            descriptionLabel.AutoEllipsis = true;
            descriptionLabel.Font = new Font(
                "Segoe UI",
                8.5F,
                FontStyle.Regular,
                GraphicsUnit.Point);
            descriptionLabel.Location = new Point(18, 40);
            descriptionLabel.Name = "CardDescription";
            descriptionLabel.Size = new Size(310, 20);
            descriptionLabel.Tag = ThemeManager.SecondaryTextRole;
            card.Controls.Add(descriptionLabel);
        }

        private static void PrepareComboBox(ComboBox comboBox, string accessibleName)
        {
            comboBox.AccessibleName = accessibleName;
            comboBox.DropDownWidth = 300;
            comboBox.Location = new Point(344, 23);
            comboBox.Size = new Size(234, 27);
        }

        private static RoundedPanel CreateSliderCard(
            string title,
            string description,
            ModernSlider slider,
            Label valueLabel)
        {
            RoundedPanel card = CreateSettingCard(title, description, 84);

            slider.AccessibleName = title;
            slider.Location = new Point(329, 25);
            slider.Size = new Size(202, 34);

            valueLabel.Location = new Point(529, 30);
            valueLabel.Size = new Size(49, 22);
            valueLabel.Tag = ThemeManager.AccentRole;
            valueLabel.TextAlign = ContentAlignment.MiddleRight;

            card.Controls.Add(slider);
            card.Controls.Add(valueLabel);
            return card;
        }

        private static RoundedPanel CreateSwitchCard(
            string title,
            string description,
            ToggleSwitch toggleSwitch)
        {
            RoundedPanel card = CreateSettingCard(title, description, 78);
            toggleSwitch.AccessibleName = title;
            toggleSwitch.Location = new Point(532, 27);
            card.Controls.Add(toggleSwitch);
            return card;
        }

        private static Label CreateValueLabel(int x, int y)
        {
            Label label = new Label();
            label.ForeColor = Color.FromArgb(35, 122, 230);
            label.Location = new Point(x, y);
            label.Size = new Size(74, 20);
            label.Tag = ThemeManager.AccentRole;
            label.TextAlign = ContentAlignment.TopRight;
            return label;
        }

        private sealed class ModeOption
        {
            internal DimmerMode Mode { get; private set; }
            private readonly string displayName;

            internal ModeOption(DimmerMode mode, string displayName)
            {
                Mode = mode;
                this.displayName = displayName;
            }

            public override string ToString()
            {
                return displayName;
            }
        }

        private sealed class MonitorOption
        {
            internal string StableId { get; private set; }
            internal string Detail { get; private set; }
            private readonly string displayName;

            internal MonitorOption(MonitorDescriptor monitor)
            {
                StableId = monitor.StableId;

                string name = string.IsNullOrEmpty(monitor.FriendlyName)
                    ? monitor.StableId
                    : monitor.FriendlyName;

                if (string.Equals(
                    monitor.StableId,
                    AppSettings.DefaultTargetStableId,
                    StringComparison.OrdinalIgnoreCase))
                {
                    name = "DELL S2417DG";
                }
                else if (name.IndexOf("Generic", StringComparison.OrdinalIgnoreCase) >= 0 ||
                    name.IndexOf("PnP", StringComparison.OrdinalIgnoreCase) >= 0)
                {
                    name = monitor.StableId;
                }

                string primary = monitor.IsPrimary ? "（主屏）" : string.Empty;

                displayName = string.Format(
                    CultureInfo.CurrentCulture,
                    "{0}{1}  ·  {2} × {3}",
                    name,
                    primary,
                    monitor.Bounds.Width,
                    monitor.Bounds.Height);

                Detail = monitor.GdiName + "  ·  " + monitor.StableId;
            }

            internal MonitorOption(string stableId, string displayName)
            {
                StableId = stableId;
                this.displayName = displayName;
                Detail = stableId;
            }

            public override string ToString()
            {
                return displayName;
            }
        }

        private sealed class ThemeOption
        {
            internal AppThemeMode Mode { get; private set; }
            private readonly string displayName;

            internal ThemeOption(AppThemeMode mode, string displayName)
            {
                Mode = mode;
                this.displayName = displayName;
            }

            public override string ToString()
            {
                return displayName;
            }
        }
    }
}
