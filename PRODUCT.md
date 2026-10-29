# fishcreen

<!-- impeccable:product-schema 1 -->

## Platform

web

Windows 桌面应用，设置面板由 Tauri / WebView2 承载，后台控制由 Rust 执行。

## Stack

用户指定 Rust + Tauri。设置面板使用 TypeScript 和原生 HTML/CSS；Vite 提供开发与打包。

## Users

在 Windows 双屏环境中使用主屏观影或日常工作的用户，希望副屏自动降低亮度、空闲时显示黑幕，返回时恢复。

## Product Purpose

根据鼠标位置和空闲时间控制所选显示器的硬件亮度及黑幕，减少副屏干扰；后台保持轻量。

## Operating Context

当前目标屏幕为 Dell S2417DG（DELA0E7），竖屏，主屏为 LG OLED TV。支持选择当前活动屏幕；所选设备断开时暂停控制，不改选其他屏幕。

## Capabilities and Constraints

- 日常、观影、自定义和关闭模式；固定预设不可被编辑，改参数进入自定义。
- 离屏变暗延迟、全局鼠标空闲黑屏时间、亮度变暗时间 0–3 秒、变暗帧数 5–60、曲线指数 1–4。
- 亮度恢复时间 0–1 秒、恢复帧数 1–30、黑幕恢复时间 0.1–5 秒；黑幕渐入为 5 秒。
- 硬件亮度渐变、黑幕渐变开关；默认黑暗主题，仍可选跟随系统、明亮。
- 托盘、开机启动开关、应用不关闭窗口、保存并关闭、立即恢复与退出恢复。
- 兼容 %LocalAppData%\\fishcreen 中的原有配置与亮度恢复记录；变更配置前备份。
- 成品默认部署到 D:\\Apps；旧版源码保留以便回退。
- DDC 每次写入有硬件延迟，参数不是实际时长的保证；本次断电后显示器恢复，触发原因尚未确认。

## Brand Commitments

名称 fishcreen，复用 assets/fishcreen.ico。界面中文。用户确认保留功能并重新设计设置面板。

## Evidence on Hand

src/ 下的现有 WinForms 实现、README.md、用户此前认可的设置面板与本任务中的功能参数。没有虚构性能跑分。

## Product Principles

- 先看清屏幕、模式与当前状态，再调整参数。
- 操作可恢复，退出和异常后保留恢复能力。
- 设置面板按需创建，关闭后销毁 WebView，后台不用 WebView 画黑幕。
- 参数含义和单位一致，保留用户配置。
