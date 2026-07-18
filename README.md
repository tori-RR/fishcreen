# fishcreen

一个轻量的 Windows 托盘程序，用于在双屏环境中自动降低副屏亮度并显示渐变黑幕：

当前版本：`1.0`

## 构建

在 PowerShell 中执行：

```powershell
.\build.ps1
```

生成文件：

```text
bin\fishcreen.exe
```

构建需要 Windows 自带的 .NET Framework 4.x 64 位 C# 编译器；程序当前编译为 x64。

## 使用

1. 双击运行 `fishcreen.exe`。
2. 程序首次启动默认使用日常模式，之后会记住上次选择。
3. 右键托盘图标可选择日常模式、观影模式或关闭自动暗屏。
4. 托盘菜单可开关“开机自动启动”、立即恢复副屏或退出。
5. 双击托盘图标可以在当前模式与关闭状态之间切换。

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

当前版本暂未包含设置窗口和全局快捷键；这些可以继续逐步增加。

“开机自动启动”默认关闭。启用后会写入当前用户的注册表启动项，不需要管理员权限：

```text
HKCU\Software\Microsoft\Windows\CurrentVersion\Run\fishcreen
```
