using System;

namespace SecondScreenDimmer
{
    internal enum AppThemeMode
    {
        System,
        Light,
        Dark
    }

    internal sealed class DimmerProfile
    {
        internal const int MinimumLeaveDelayMilliseconds = 500;
        internal const int MaximumLeaveDelayMilliseconds = 10000;
        internal const int MinimumIdleBlackoutMilliseconds = 5000;
        internal const int MaximumIdleBlackoutMilliseconds = 300000;
        internal const int MinimumBrightnessFadeSteps = 5;
        internal const int MaximumBrightnessFadeSteps = 60;
        internal const double MinimumBrightnessFadeExponent = 1.0;
        internal const double MaximumBrightnessFadeExponent = 4.0;

        internal int LeaveDelayMilliseconds { get; set; }
        internal int IdleBlackoutMilliseconds { get; set; }
        internal bool BrightnessFadeEnabled { get; set; }
        internal bool BlackoutFadeEnabled { get; set; }
        internal int BrightnessFadeSteps { get; set; }
        internal double BrightnessFadeExponent { get; set; }

        internal DimmerProfile Clone()
        {
            return new DimmerProfile
            {
                LeaveDelayMilliseconds = LeaveDelayMilliseconds,
                IdleBlackoutMilliseconds = IdleBlackoutMilliseconds,
                BrightnessFadeEnabled = BrightnessFadeEnabled,
                BlackoutFadeEnabled = BlackoutFadeEnabled,
                BrightnessFadeSteps = BrightnessFadeSteps,
                BrightnessFadeExponent = BrightnessFadeExponent
            };
        }

        internal void Normalize()
        {
            LeaveDelayMilliseconds = Math.Max(
                MinimumLeaveDelayMilliseconds,
                Math.Min(MaximumLeaveDelayMilliseconds, LeaveDelayMilliseconds));

            IdleBlackoutMilliseconds = Math.Max(
                MinimumIdleBlackoutMilliseconds,
                Math.Min(MaximumIdleBlackoutMilliseconds, IdleBlackoutMilliseconds));

            BrightnessFadeSteps = Math.Max(
                MinimumBrightnessFadeSteps,
                Math.Min(MaximumBrightnessFadeSteps, BrightnessFadeSteps));

            BrightnessFadeExponent = Math.Max(
                MinimumBrightnessFadeExponent,
                Math.Min(MaximumBrightnessFadeExponent, BrightnessFadeExponent));
        }
    }

    internal sealed class AppSettings
    {
        internal const string DefaultTargetStableId = "DELA0E7";

        internal string TargetStableId { get; set; }
        internal DimmerMode Mode { get; set; }
        internal AppThemeMode ThemeMode { get; set; }
        internal DimmerProfile CustomProfile { get; set; }

        internal AppSettings()
        {
            TargetStableId = DefaultTargetStableId;
            Mode = DimmerMode.Daily;
            ThemeMode = AppThemeMode.System;
            CustomProfile = CreateDailyProfile();
        }

        internal AppSettings Clone()
        {
            return new AppSettings
            {
                TargetStableId = TargetStableId,
                Mode = Mode,
                ThemeMode = ThemeMode,
                CustomProfile = CustomProfile == null
                    ? CreateDailyProfile()
                    : CustomProfile.Clone()
            };
        }

        internal void Normalize()
        {
            TargetStableId = (TargetStableId ?? string.Empty).Trim();

            if (!Enum.IsDefined(typeof(DimmerMode), Mode))
            {
                Mode = DimmerMode.Daily;
            }

            if (!Enum.IsDefined(typeof(AppThemeMode), ThemeMode))
            {
                ThemeMode = AppThemeMode.System;
            }

            if (CustomProfile == null)
            {
                CustomProfile = CreateDailyProfile();
            }

            CustomProfile.Normalize();
        }

        internal DimmerProfile GetEffectiveProfile()
        {
            switch (Mode)
            {
                case DimmerMode.Movie:
                    return CreateMovieProfile();
                case DimmerMode.Custom:
                    return CustomProfile.Clone();
                default:
                    return CreateDailyProfile();
            }
        }

        internal static DimmerProfile CreateDailyProfile()
        {
            return new DimmerProfile
            {
                LeaveDelayMilliseconds = 1500,
                IdleBlackoutMilliseconds = 30000,
                BrightnessFadeEnabled = true,
                BlackoutFadeEnabled = true,
                BrightnessFadeSteps = 30,
                BrightnessFadeExponent = 2.4
            };
        }

        internal static DimmerProfile CreateMovieProfile()
        {
            return new DimmerProfile
            {
                LeaveDelayMilliseconds = 1500,
                IdleBlackoutMilliseconds = 30000,
                BrightnessFadeEnabled = true,
                BlackoutFadeEnabled = false,
                BrightnessFadeSteps = 30,
                BrightnessFadeExponent = 2.4
            };
        }
    }
}
