[CmdletBinding()]
param(
    [string]$Executable = 'D:\Apps\fishcreen.exe',
    [string]$Output = (Join-Path $PSScriptRoot '..\bin\icon-audit\embedded-icon.png')
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public static class FishcreenIconAudit {
    private delegate bool ResourceCallback(IntPtr module, IntPtr type, IntPtr name, IntPtr parameter);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr LoadLibraryExW(string path, IntPtr file, uint flags);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool EnumResourceNamesW(IntPtr module, IntPtr type, ResourceCallback callback, IntPtr parameter);
    [DllImport("kernel32.dll")]
    public static extern bool FreeLibrary(IntPtr module);
    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr LoadImageW(IntPtr module, IntPtr name, uint type, int width, int height, uint flags);
    [DllImport("user32.dll")]
    public static extern bool DestroyIcon(IntPtr icon);

    public static IntPtr LoadEmbeddedIcon(string path) {
        IntPtr module = LoadLibraryExW(path, IntPtr.Zero, 0x22);
        if (module == IntPtr.Zero) throw new System.ComponentModel.Win32Exception();
        try {
            IntPtr icon = IntPtr.Zero;
            ResourceCallback callback = (m, t, name, p) => {
                icon = LoadImageW(m, name, 1, 256, 256, 0);
                return false;
            };
            EnumResourceNamesW(module, new IntPtr(14), callback, IntPtr.Zero);
            GC.KeepAlive(callback);
            if (icon == IntPtr.Zero) throw new InvalidOperationException("No embedded icon resource found.");
            return icon;
        } finally { FreeLibrary(module); }
    }
}
'@

$resolvedOutput = [IO.Path]::GetFullPath($Output)
New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($resolvedOutput)) -Force | Out-Null
$iconHandle = [FishcreenIconAudit]::LoadEmbeddedIcon([IO.Path]::GetFullPath($Executable))
try {
    $bitmap = [Drawing.Icon]::FromHandle($iconHandle).ToBitmap()
    try { $bitmap.Save($resolvedOutput, [Drawing.Imaging.ImageFormat]::Png) }
    finally { $bitmap.Dispose() }
} finally { [FishcreenIconAudit]::DestroyIcon($iconHandle) | Out-Null }
Get-Item -LiteralPath $resolvedOutput | Select-Object FullName,Length
