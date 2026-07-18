using System;
using System.Runtime.InteropServices;

namespace SecondScreenDimmer
{
    internal sealed class OperationResult
    {
        internal bool Success { get; private set; }
        internal string Message { get; private set; }

        private OperationResult(bool success, string message)
        {
            Success = success;
            Message = message;
        }

        internal static OperationResult Ok(string message)
        {
            return new OperationResult(true, message);
        }

        internal static OperationResult Fail(string message)
        {
            return new OperationResult(false, message);
        }
    }

    internal sealed class BrightnessService
    {
        private readonly object syncRoot = new object();
        private readonly MonitorService monitorService;
        private readonly string targetStableId;

        private bool hasOriginalBrightness;
        private uint originalBrightness;

        internal BrightnessService(MonitorService monitorService, string targetStableId)
        {
            this.monitorService = monitorService;
            this.targetStableId = targetStableId;
        }

        internal OperationResult TryRecoverFromPreviousRun()
        {
            lock (syncRoot)
            {
                uint savedBrightness;
                if (!RecoveryStore.TryLoad(targetStableId, out savedBrightness))
                {
                    return OperationResult.Ok("无需恢复");
                }

                originalBrightness = savedBrightness;
                hasOriginalBrightness = true;
                return RestoreCore();
            }
        }

        internal OperationResult DimTo(uint requestedBrightness)
        {
            lock (syncRoot)
            {
                MonitorDescriptor target = monitorService.FindByStableId(targetStableId);
                if (target == null)
                {
                    return OperationResult.Fail("未找到 Dell 显示器");
                }

                NativeMethods.PhysicalMonitor[] physicalMonitors;
                string openError;
                if (!TryOpenPhysicalMonitors(target.Handle, out physicalMonitors, out openError))
                {
                    return OperationResult.Fail(openError);
                }

                try
                {
                    if (physicalMonitors.Length == 0)
                    {
                        return OperationResult.Fail("没有可用的物理显示器句柄");
                    }

                    uint minimum;
                    uint current;
                    uint maximum;
                    if (!NativeMethods.GetMonitorBrightness(
                        physicalMonitors[0].Handle,
                        out minimum,
                        out current,
                        out maximum))
                    {
                        return OperationResult.Fail(
                            "读取 Dell 亮度失败：" + Marshal.GetLastWin32Error());
                    }

                    bool capturedNow = false;
                    if (!hasOriginalBrightness)
                    {
                        originalBrightness = current;
                        hasOriginalBrightness = true;
                        capturedNow = true;
                        RecoveryStore.Save(targetStableId, originalBrightness);
                    }

                    uint brightness = Math.Max(minimum, Math.Min(requestedBrightness, maximum));
                    if (!NativeMethods.SetMonitorBrightness(physicalMonitors[0].Handle, brightness))
                    {
                        int error = Marshal.GetLastWin32Error();
                        if (capturedNow)
                        {
                            hasOriginalBrightness = false;
                            RecoveryStore.Clear();
                        }

                        return OperationResult.Fail("设置 Dell 亮度失败：" + error);
                    }

                    AppLog.Write("Dell brightness dimmed from " + current + " to " + brightness + ".");
                    return OperationResult.Ok("Dell 亮度已降至 " + brightness);
                }
                finally
                {
                    NativeMethods.DestroyPhysicalMonitors(
                        (uint)physicalMonitors.Length,
                        physicalMonitors);
                }
            }
        }

        internal OperationResult Restore()
        {
            lock (syncRoot)
            {
                if (!hasOriginalBrightness)
                {
                    uint savedBrightness;
                    if (!RecoveryStore.TryLoad(targetStableId, out savedBrightness))
                    {
                        return OperationResult.Ok("亮度已经恢复");
                    }

                    originalBrightness = savedBrightness;
                    hasOriginalBrightness = true;
                }

                return RestoreCore();
            }
        }

        private OperationResult RestoreCore()
        {
            MonitorDescriptor target = monitorService.FindByStableId(targetStableId);
            if (target == null)
            {
                return OperationResult.Fail("恢复失败：未找到 Dell 显示器");
            }

            NativeMethods.PhysicalMonitor[] physicalMonitors;
            string openError;
            if (!TryOpenPhysicalMonitors(target.Handle, out physicalMonitors, out openError))
            {
                return OperationResult.Fail("恢复失败：" + openError);
            }

            try
            {
                if (physicalMonitors.Length == 0)
                {
                    return OperationResult.Fail("恢复失败：没有物理显示器句柄");
                }

                if (!NativeMethods.SetMonitorBrightness(
                    physicalMonitors[0].Handle,
                    originalBrightness))
                {
                    return OperationResult.Fail(
                        "恢复 Dell 亮度失败：" + Marshal.GetLastWin32Error());
                }

                uint restored = originalBrightness;
                hasOriginalBrightness = false;
                RecoveryStore.Clear();
                AppLog.Write("Dell brightness restored to " + restored + ".");
                return OperationResult.Ok("Dell 亮度已恢复到 " + restored);
            }
            finally
            {
                NativeMethods.DestroyPhysicalMonitors(
                    (uint)physicalMonitors.Length,
                    physicalMonitors);
            }
        }

        private static bool TryOpenPhysicalMonitors(
            IntPtr monitor,
            out NativeMethods.PhysicalMonitor[] physicalMonitors,
            out string error)
        {
            physicalMonitors = null;
            error = string.Empty;

            uint count;
            if (!NativeMethods.GetNumberOfPhysicalMonitorsFromHMONITOR(monitor, out count))
            {
                error = "枚举物理显示器失败：" + Marshal.GetLastWin32Error();
                return false;
            }

            physicalMonitors = new NativeMethods.PhysicalMonitor[count];
            if (!NativeMethods.GetPhysicalMonitorsFromHMONITOR(
                monitor,
                count,
                physicalMonitors))
            {
                error = "打开物理显示器失败：" + Marshal.GetLastWin32Error();
                physicalMonitors = null;
                return false;
            }

            return true;
        }
    }
}
