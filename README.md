# fishcreen

一个轻量的 Windows 托盘程序，用于在双屏环境中自动降低副屏亮度并显示渐变黑幕：

当前版本：`1.1.1`（Rust + Tauri 重写预览）

## 运行环境

Windows x64、Microsoft Edge WebView2 Runtime。硬件调光需要显示器支持并启用 DDC/CI。

## 构建

在 PowerShell 中执行：

```powershell
.\build.ps1
```

生成文件：

```text
D:\Apps\fishcreen.exe
```

构建需要 Rust（MSVC x64 工具链）、Visual Studio C++ Build Tools、Windows SDK 和 Node.js/npm。首次构建会安装前端依赖。

默认输出到 `D:\Apps`，已有程序会先备份；可用 `-OutputDirectory` 指定其他目录，或用 `-NoDeploy` 仅生成 `src-tauri\target\release\fishcreen.exe`。

旧版 C# 源码保留，可用 `.\build-legacy.ps1` 构建。

## 使用

1. 双击运行 `fishcreen.exe`。
2. 程序首次启动默认使用日常模式，之后会记住上次选择。
3. 左键单击托盘图标可打开设置窗口，选择生效屏幕和运行模式。
4. 日常模式与观影模式使用固定预设；调整任意参数会自动切换到自定义模式。
5. 自定义模式可设置离屏变暗时间、无动作黑屏时间、黑幕恢复时间、亮度变暗时间与帧数、亮度恢复时间与帧数、亮度曲线指数，以及亮度/黑屏渐变开关。
6. 界面默认黑暗，支持跟随系统、明亮和黑暗三种模式；已有设置会保留。
7. 右键托盘图标可快速切换模式、开关“开机自动启动”、立即恢复副屏或退出。

设置窗口关闭后销毁 WebView，调光和黑幕由 Rust/Win32 后台继续运行。硬件响应速度可能使实际亮度过渡长于设定时间。

## 恢复保护

暗屏前的亮度会临时记录在：

```text
%LocalAppData%\fishcreen\brightness-recovery.txt
```

正常恢复后记录会被删除。如果程序异常退出，下次启动时会优先尝试恢复记录的亮度。

运行日志位于：

```text
%LocalAppData%\fishcreen\app.log
```

设置保存在：

```text
%LocalAppData%\fishcreen\settings.txt
```

沿用旧版设置和亮度恢复记录；首次保存前会将原设置备份为 `settings.before-tauri.txt`。

当前版本暂未包含全局快捷键；这个功能可以继续逐步增加。

“开机自动启动”默认关闭。启用后会写入当前用户的注册表启动项，不需要管理员权限：

```text
HKCU\Software\Microsoft\Windows\CurrentVersion\Run\fishcreen
```
