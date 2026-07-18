using System;
using System.Drawing;
using System.Threading;
using System.Threading.Tasks;
using System.Windows.Forms;

namespace SecondScreenDimmer
{
    internal sealed class ScreenDimmerController : IDisposable
    {
        internal const string TargetStableId = "DELA0E7";
        private const uint DimBrightnessValue = 0;
        private const int LeaveDelayMilliseconds = 1500;
        private const int BrightnessFadeDurationMilliseconds = 300;
        private const int DailyFadeStartMilliseconds = 30000;
        private const int DailyFadeDurationMilliseconds = 5000;
        private const int MonitorRefreshMilliseconds = 3000;

        private readonly MonitorService monitorService;
        private readonly BrightnessService brightnessService;
        private readonly OverlayForm overlay;
        private readonly System.Windows.Forms.Timer cursorTimer;
        private readonly object queueSync = new object();

        private Task brightnessQueue;
        private CancellationTokenSource brightnessFadeCancellation;
        private MonitorDescriptor target;
        private DateTime? outsideSince;
        private DateTime lastMouseMovementAt;
        private DateTime nextMonitorRefresh;
        private NativeMethods.Point previousCursor;
        private bool hasPreviousCursor;
        private bool brightnessDimmed;
        private bool overlayVisible;
        private bool dailyFadeCompleted;
        private double overlayOpacity;
        private bool disposed;
        private DimmerMode mode;

        internal event Action<string> StatusChanged;
        internal event Action<DimmerMode> ModeChanged;

        internal DimmerMode Mode
        {
            get { return mode; }
        }

        internal ScreenDimmerController()
        {
            monitorService = new MonitorService();
            brightnessService = new BrightnessService(monitorService, TargetStableId);
            overlay = new OverlayForm();

            // Create the native handle on the UI thread without showing the form.
            IntPtr unused = overlay.Handle;
            brightnessQueue = Task.FromResult(0);

            cursorTimer = new System.Windows.Forms.Timer();
            cursorTimer.Interval = 75;
            cursorTimer.Tick += OnCursorTimerTick;

            mode = SettingsStore.LoadMode();
            lastMouseMovementAt = DateTime.UtcNow;
            RefreshTarget();
        }

        internal void Start()
        {
            OperationResult recovery = brightnessService.TryRecoverFromPreviousRun();
            AppLog.Write("Startup recovery: " + recovery.Message);

            cursorTimer.Start();
            PublishModeChanged();
            PublishStatus(GetModeStatus());
        }

        internal void SetMode(DimmerMode newMode)
        {
            if (disposed || mode == newMode)
            {
                return;
            }

            mode = newMode;
            SettingsStore.SaveMode(mode);
            ResetActivityTimers();
            HideOverlay();
            RestoreBrightness(false);

            if (mode != DimmerMode.Off)
            {
                RefreshTarget();
            }

            PublishModeChanged();
            PublishStatus(GetModeStatus());
        }

        internal void RefreshTarget()
        {
            target = monitorService.FindByStableId(TargetStableId);
            nextMonitorRefresh = DateTime.UtcNow.AddMilliseconds(MonitorRefreshMilliseconds);

            if (target == null)
            {
                HideOverlay();
                RestoreBrightness(false);
                PublishStatus("未找到 Dell S2417DG");
            }
            else if (overlayVisible)
            {
                overlay.Cover(target.Bounds, overlayOpacity);
            }
        }

        internal void RequestRefreshTarget()
        {
            RunOnUiThread(new Action(RefreshTarget), false);
        }

        internal void ForceRestore()
        {
            ResetActivityTimers();
            HideOverlay();
            RestoreBrightness(true);
        }

        internal void RequestForceRestore()
        {
            RunOnUiThread(new Action(ForceRestore), false);
        }

        internal void RequestStopAndRestore()
        {
            RunOnUiThread(new Action(StopAndRestore), true);
        }

        internal void StopAndRestore()
        {
            if (disposed)
            {
                return;
            }

            cursorTimer.Stop();
            outsideSince = null;
            CancelBrightnessFade();
            brightnessDimmed = false;
            HideOverlay();

            Task restoreTask = EnqueueBrightness(delegate
            {
                return brightnessService.Restore();
            });

            try
            {
                restoreTask.Wait(5000);
            }
            catch (Exception exception)
            {
                AppLog.Write("Exit restore error: " + exception);
            }
        }

        private void OnCursorTimerTick(object sender, EventArgs eventArgs)
        {
            if (disposed || mode == DimmerMode.Off)
            {
                return;
            }

            DateTime now = DateTime.UtcNow;
            if (now >= nextMonitorRefresh)
            {
                RefreshTarget();
            }

            if (target == null)
            {
                return;
            }

            NativeMethods.Point cursor;
            if (!NativeMethods.GetCursorPos(out cursor))
            {
                return;
            }

            bool mouseMoved = !hasPreviousCursor ||
                cursor.X != previousCursor.X ||
                cursor.Y != previousCursor.Y;

            if (mouseMoved)
            {
                previousCursor = cursor;
                hasPreviousCursor = true;
                lastMouseMovementAt = now;

                if (mode == DimmerMode.Daily)
                {
                    HideOverlay();
                }
            }

            bool isInsideTarget = target.Bounds.Contains(new Point(cursor.X, cursor.Y));

            if (mode == DimmerMode.Movie)
            {
                ProcessMovieMode(now, isInsideTarget);
            }
            else
            {
                ProcessDailyMode(now, isInsideTarget);
            }
        }

        private void ProcessMovieMode(DateTime now, bool isInsideTarget)
        {
            if (isInsideTarget)
            {
                outsideSince = null;
                HideOverlay();
                RestoreBrightness(false);
                return;
            }

            if (!outsideSince.HasValue)
            {
                outsideSince = now;
                return;
            }

            if ((now - outsideSince.Value).TotalMilliseconds >= LeaveDelayMilliseconds)
            {
                DimBrightness();
                ShowMovieOverlay();
            }
        }

        private void ProcessDailyMode(DateTime now, bool isInsideTarget)
        {
            if (isInsideTarget)
            {
                outsideSince = null;

                if ((now - lastMouseMovementAt).TotalMilliseconds < DailyFadeStartMilliseconds)
                {
                    RestoreBrightness(false);
                }
            }
            else
            {
                if (!outsideSince.HasValue)
                {
                    outsideSince = now;
                }
                else if ((now - outsideSince.Value).TotalMilliseconds >= LeaveDelayMilliseconds)
                {
                    DimBrightness();
                }
            }

            double idleMilliseconds = (now - lastMouseMovementAt).TotalMilliseconds;
            if (idleMilliseconds >= DailyFadeStartMilliseconds)
            {
                DimBrightness();
                double progress = (idleMilliseconds - DailyFadeStartMilliseconds) /
                    DailyFadeDurationMilliseconds;
                UpdateDailyFade(progress);
            }
        }

        private void DimBrightness()
        {
            if (brightnessDimmed)
            {
                return;
            }

            brightnessDimmed = true;
            CancellationTokenSource fadeCancellation = BeginBrightnessFade();
            PublishStatus("Dell 亮度正在渐暗…");

            EnqueueBrightness(delegate
            {
                try
                {
                    return brightnessService.FadeTo(
                        DimBrightnessValue,
                        BrightnessFadeDurationMilliseconds,
                        fadeCancellation.Token);
                }
                finally
                {
                    ReleaseBrightnessFade(fadeCancellation);
                }
            });
        }

        private void RestoreBrightness(bool userRequested)
        {
            CancelBrightnessFade();
            bool wasDimmed = brightnessDimmed;
            brightnessDimmed = false;

            if (!wasDimmed && !userRequested)
            {
                return;
            }

            PublishStatus("正在恢复副屏亮度…");
            EnqueueBrightness(delegate
            {
                return brightnessService.Restore();
            });
        }

        private void ShowMovieOverlay()
        {
            if (target == null)
            {
                return;
            }

            bool newlyVisible = !overlayVisible;
            overlayVisible = true;
            dailyFadeCompleted = false;
            overlayOpacity = 1.0;
            overlay.Cover(target.Bounds, overlayOpacity);

            if (newlyVisible)
            {
                AppLog.Write("Movie mode overlay shown after leaving Dell.");
                PublishStatus("观影模式：副屏已黑屏");
            }
        }

        private void UpdateDailyFade(double progress)
        {
            if (target == null)
            {
                return;
            }

            double clampedProgress = Math.Max(0.0, Math.Min(1.0, progress));
            bool starting = !overlayVisible;
            overlayVisible = true;
            overlayOpacity = clampedProgress;

            if (starting)
            {
                dailyFadeCompleted = false;
                overlay.Cover(target.Bounds, overlayOpacity);
                AppLog.Write("Daily mode overlay fade started at 30 seconds idle.");
                PublishStatus("日常模式：副屏正在渐暗");
            }
            else
            {
                overlay.SetOverlayOpacity(overlayOpacity);
            }

            if (clampedProgress >= 1.0 && !dailyFadeCompleted)
            {
                dailyFadeCompleted = true;
                AppLog.Write("Daily mode overlay fade completed at 35 seconds idle.");
                PublishStatus("日常模式：副屏已黑屏");
            }
        }

        private void HideOverlay()
        {
            if (!overlayVisible)
            {
                return;
            }

            overlayVisible = false;
            dailyFadeCompleted = false;
            overlayOpacity = 0.0;
            overlay.Uncover();
            AppLog.Write("Overlay hidden.");
        }

        private void ResetActivityTimers()
        {
            outsideSince = null;
            lastMouseMovementAt = DateTime.UtcNow;
            hasPreviousCursor = false;
        }

        private string GetModeStatus()
        {
            if (target == null)
            {
                return "未找到 Dell S2417DG";
            }

            switch (mode)
            {
                case DimmerMode.Daily:
                    return "日常模式已启用";
                case DimmerMode.Movie:
                    return "观影模式已启用";
                default:
                    return "自动暗屏已关闭";
            }
        }

        private Task EnqueueBrightness(Func<OperationResult> operation)
        {
            lock (queueSync)
            {
                brightnessQueue = brightnessQueue.ContinueWith(
                    delegate(Task previous)
                    {
                        try
                        {
                            OperationResult result = operation();
                            AppLog.Write(result.Message);
                            PublishStatusFromWorker(result.Message);
                        }
                        catch (Exception exception)
                        {
                            AppLog.Write("Brightness operation failed: " + exception);
                            PublishStatusFromWorker("亮度操作失败，详情见日志");
                        }
                    },
                    CancellationToken.None,
                    TaskContinuationOptions.None,
                    TaskScheduler.Default);

                return brightnessQueue;
            }
        }

        private CancellationTokenSource BeginBrightnessFade()
        {
            lock (queueSync)
            {
                if (brightnessFadeCancellation != null)
                {
                    brightnessFadeCancellation.Cancel();
                }

                brightnessFadeCancellation = new CancellationTokenSource();
                return brightnessFadeCancellation;
            }
        }

        private void CancelBrightnessFade()
        {
            lock (queueSync)
            {
                if (brightnessFadeCancellation != null)
                {
                    brightnessFadeCancellation.Cancel();
                }
            }
        }

        private void ReleaseBrightnessFade(CancellationTokenSource fadeCancellation)
        {
            lock (queueSync)
            {
                if (ReferenceEquals(brightnessFadeCancellation, fadeCancellation))
                {
                    brightnessFadeCancellation = null;
                }
            }

            fadeCancellation.Dispose();
        }

        private void PublishStatusFromWorker(string status)
        {
            try
            {
                if (!overlay.IsDisposed)
                {
                    overlay.BeginInvoke(new Action<string>(PublishStatus), status);
                }
            }
            catch
            {
                // The UI may be closing while the last DDC operation completes.
            }
        }

        private void RunOnUiThread(Action action, bool wait)
        {
            try
            {
                if (overlay.IsDisposed)
                {
                    return;
                }

                if (!overlay.InvokeRequired)
                {
                    action();
                }
                else if (wait)
                {
                    overlay.Invoke(action);
                }
                else
                {
                    overlay.BeginInvoke(action);
                }
            }
            catch (Exception exception)
            {
                AppLog.Write("UI dispatch failed: " + exception.Message);
            }
        }

        private void PublishStatus(string status)
        {
            Action<string> handler = StatusChanged;
            if (handler != null)
            {
                handler(status);
            }
        }

        private void PublishModeChanged()
        {
            Action<DimmerMode> handler = ModeChanged;
            if (handler != null)
            {
                handler(mode);
            }
        }

        public void Dispose()
        {
            if (disposed)
            {
                return;
            }

            StopAndRestore();
            disposed = true;
            cursorTimer.Dispose();
            overlay.Dispose();
        }
    }
}
