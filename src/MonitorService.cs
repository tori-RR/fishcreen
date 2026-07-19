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
        internal string StableId { get; private set; }
        internal string FriendlyName { get; private set; }
        internal bool IsPrimary { get; private set; }

        internal MonitorDescriptor(
            IntPtr handle,
            Rectangle bounds,
            string gdiName,
            string deviceInterfaceId,
            string stableId,
            string friendlyName,
            bool isPrimary)
        {
            Handle = handle;
            Bounds = bounds;
            GdiName = gdiName;
            DeviceInterfaceId = deviceInterfaceId;
            StableId = stableId;
            FriendlyName = friendlyName;
            IsPrimary = isPrimary;
        }
    }

    internal sealed class MonitorService
    {
        internal MonitorDescriptor FindByStableId(string stableId)
        {
            List<MonitorDescriptor> monitors = EnumerateActiveMonitors(false);

            foreach (MonitorDescriptor monitor in monitors)
            {
                if (string.Equals(monitor.StableId, stableId, StringComparison.OrdinalIgnoreCase) ||
                    (!string.IsNullOrEmpty(monitor.DeviceInterfaceId) &&
                    monitor.DeviceInterfaceId.IndexOf(stableId, StringComparison.OrdinalIgnoreCase) >= 0))
                {
                    return monitor;
                }
            }

            return null;
        }

        internal List<MonitorDescriptor> EnumerateActiveMonitors()
        {
            return EnumerateActiveMonitors(true);
        }

        private static List<MonitorDescriptor> EnumerateActiveMonitors(
            bool includePhysicalDescription)
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
                string friendlyName = string.Empty;
                if (NativeMethods.EnumDisplayDevices(
                    info.DeviceName,
                    0,
                    ref device,
                    NativeMethods.EddGetDeviceInterfaceName))
                {
                    interfaceId = device.DeviceId ?? string.Empty;
                    friendlyName = device.DeviceString ?? string.Empty;
                }

                string stableId = ExtractStableId(interfaceId, info.DeviceName);
                if (includePhysicalDescription)
                {
                    string physicalDescription = GetPhysicalMonitorDescription(monitor);
                    if (!string.IsNullOrEmpty(physicalDescription))
                    {
                        friendlyName = physicalDescription;
                    }
                }

                result.Add(new MonitorDescriptor(
                    monitor,
                    info.Monitor.ToRectangle(),
                    info.DeviceName,
                    interfaceId,
                    stableId,
                    friendlyName,
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

        private static string GetPhysicalMonitorDescription(IntPtr monitor)
        {
            uint count;
            if (!NativeMethods.GetNumberOfPhysicalMonitorsFromHMONITOR(monitor, out count) ||
                count == 0)
            {
                return string.Empty;
            }

            NativeMethods.PhysicalMonitor[] physicalMonitors =
                new NativeMethods.PhysicalMonitor[count];

            if (!NativeMethods.GetPhysicalMonitorsFromHMONITOR(
                monitor,
                count,
                physicalMonitors))
            {
                return string.Empty;
            }

            try
            {
                return physicalMonitors[0].Description ?? string.Empty;
            }
            finally
            {
                NativeMethods.DestroyPhysicalMonitors(count, physicalMonitors);
            }
        }

        private static string ExtractStableId(string interfaceId, string fallback)
        {
            if (!string.IsNullOrEmpty(interfaceId))
            {
                string[] parts = interfaceId.Split('#');
                if (parts.Length >= 2 && !string.IsNullOrEmpty(parts[1]))
                {
                    return parts[1];
                }
            }

            return fallback ?? string.Empty;
        }
    }
}
