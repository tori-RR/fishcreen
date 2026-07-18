using System;
using System.Collections.Generic;
using System.Drawing;
using System.Runtime.InteropServices;

namespace SecondScreenDimmer
{
    internal sealed class MonitorDescriptor
    {
        internal IntPtr Handle { get; private set; }
        internal Rectangle Bounds { get; private set; }
        internal string GdiName { get; private set; }
        internal string DeviceInterfaceId { get; private set; }
        internal bool IsPrimary { get; private set; }

        internal MonitorDescriptor(
            IntPtr handle,
            Rectangle bounds,
            string gdiName,
            string deviceInterfaceId,
            bool isPrimary)
        {
            Handle = handle;
            Bounds = bounds;
            GdiName = gdiName;
            DeviceInterfaceId = deviceInterfaceId;
            IsPrimary = isPrimary;
        }
    }

    internal sealed class MonitorService
    {
        internal MonitorDescriptor FindByStableId(string stableId)
        {
            List<MonitorDescriptor> monitors = EnumerateActiveMonitors();

            foreach (MonitorDescriptor monitor in monitors)
            {
                if (!string.IsNullOrEmpty(monitor.DeviceInterfaceId) &&
                    monitor.DeviceInterfaceId.IndexOf(stableId, StringComparison.OrdinalIgnoreCase) >= 0)
                {
                    return monitor;
                }
            }

            return null;
        }

        private static List<MonitorDescriptor> EnumerateActiveMonitors()
        {
            List<MonitorDescriptor> result = new List<MonitorDescriptor>();

            NativeMethods.MonitorEnumProc callback = delegate(
                IntPtr monitor,
                IntPtr deviceContext,
                IntPtr monitorRectangle,
                IntPtr userData)
            {
                NativeMethods.MonitorInfoEx info = new NativeMethods.MonitorInfoEx();
                info.Size = Marshal.SizeOf(typeof(NativeMethods.MonitorInfoEx));

                if (!NativeMethods.GetMonitorInfo(monitor, ref info))
                {
                    return true;
                }

                NativeMethods.DisplayDevice device = new NativeMethods.DisplayDevice();
                device.Size = Marshal.SizeOf(typeof(NativeMethods.DisplayDevice));

                string interfaceId = string.Empty;
                if (NativeMethods.EnumDisplayDevices(
                    info.DeviceName,
                    0,
                    ref device,
                    NativeMethods.EddGetDeviceInterfaceName))
                {
                    interfaceId = device.DeviceId ?? string.Empty;
                }

                result.Add(new MonitorDescriptor(
                    monitor,
                    info.Monitor.ToRectangle(),
                    info.DeviceName,
                    interfaceId,
                    (info.Flags & 1) != 0));

                return true;
            };

            NativeMethods.EnumDisplayMonitors(
                IntPtr.Zero,
                IntPtr.Zero,
                callback,
                IntPtr.Zero);

            GC.KeepAlive(callback);
            return result;
        }
    }
}
