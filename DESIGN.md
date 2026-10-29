---
name: fishcreen
description: Windows 副屏亮度与黑幕控制工具的设置界面。
colors:
  dark-bg: "#111419"
  dark-panel: "#1b2028"
  dark-text: "#e5e9f0"
  dark-muted: "#a4aebd"
  dark-line: "#2a313d"
  dark-input: "#252c37"
  dark-hover: "#303a48"
  dark-accent: "#8db6ed"
  dark-on-accent: "#152438"
  dark-accent-soft: "#233a56"
  dark-track: "#3a4555"
  dark-thumb: "#eff4fc"
  dark-success: "#92c8a6"
  dark-danger: "#ffb3b7"
  light-bg: "#f4f5f7"
  light-panel: "#fff"
  light-text: "#20252e"
  light-muted: "#626c7b"
  light-line: "#e8ebef"
  light-input: "#f2f4f7"
  light-hover: "#e9edf3"
  light-accent: "#366bb7"
  light-on-accent: "#fff"
  light-accent-soft: "#e7eef9"
  light-track: "#d9dfe7"
  light-thumb: "#fff"
  light-success: "#347851"
  light-danger: "#a72e35"
typography:
  headline:
    fontFamily: "'Segoe UI Variable', 'Segoe UI', 'Microsoft YaHei UI', sans-serif"
    fontSize: "27px"
    fontWeight: 650
    lineHeight: 1.5
    letterSpacing: "-0.025em"
  title:
    fontFamily: "'Segoe UI Variable', 'Segoe UI', 'Microsoft YaHei UI', sans-serif"
    fontSize: "13px"
    fontWeight: 650
    lineHeight: 1.5
    letterSpacing: "0.015em"
  body:
    fontFamily: "'Segoe UI Variable', 'Segoe UI', 'Microsoft YaHei UI', sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.5
  field-label:
    fontFamily: "'Segoe UI Variable', 'Segoe UI', 'Microsoft YaHei UI', sans-serif"
    fontSize: "14px"
    fontWeight: 500
    lineHeight: 1.5
  label:
    fontFamily: "'Segoe UI Variable', 'Segoe UI', 'Microsoft YaHei UI', sans-serif"
    fontSize: "12px"
    fontWeight: 600
    lineHeight: 1.5
  caption:
    fontFamily: "'Segoe UI Variable', 'Segoe UI', 'Microsoft YaHei UI', sans-serif"
    fontSize: "11.5px"
    fontWeight: 400
    lineHeight: 1.6
rounded:
  number: "6px"
  select: "7px"
  button: "8px"
  message: "10px"
  switch: "12px"
  section: "14px"
spacing:
  detail: "3px"
  button-block: "7px"
  compact: "9px"
  control-gap: "12px"
  row-block: "17px"
  row-inline: "21px"
  section-gap: "27px"
  page-inline: "36px"
components:
  button-primary:
    backgroundColor: "{colors.dark-accent}"
    textColor: "{colors.dark-on-accent}"
    typography: "{typography.label}"
    rounded: "{rounded.button}"
    padding: "7px 17px"
  button-secondary:
    backgroundColor: "{colors.dark-input}"
    textColor: "{colors.dark-text}"
    typography: "{typography.label}"
    rounded: "{rounded.button}"
    padding: "7px 17px"
  button-secondary-hover:
    backgroundColor: "{colors.dark-hover}"
  button-text:
    backgroundColor: "transparent"
    textColor: "{colors.dark-accent}"
    typography: "{typography.label}"
    rounded: "{rounded.button}"
    padding: "7px 10px"
  button-text-hover:
    backgroundColor: "{colors.dark-accent-soft}"
  select:
    backgroundColor: "{colors.dark-input}"
    textColor: "{colors.dark-text}"
    rounded: "{rounded.select}"
    padding: "8px 10px"
    width: "217px"
  number-input:
    backgroundColor: "transparent"
    textColor: "{colors.dark-text}"
    rounded: "{rounded.number}"
    padding: "6px 3px"
    width: "48px"
  section-panel:
    backgroundColor: "{colors.dark-panel}"
    rounded: "{rounded.section}"
  switch:
    backgroundColor: "{colors.dark-track}"
    rounded: "{rounded.switch}"
    width: "38px"
    height: "22px"
  switch-checked:
    backgroundColor: "{colors.dark-accent}"
---

# Design System: fishcreen

## Overview

**Creative North Star: "安静的副屏调节台"**

界面的表达来自现有实现：墨色背景承托低对比度分组，蓝色标识可操作和已选状态，信息沿设置行依次展开。它是一个 Windows 操作面板，不是展示页；屏幕、模式、参数和保存状态比装饰更重要。

默认外观为黑暗，同时保留明亮与跟随系统。两套主题共享组件尺寸、字阶与交互结构，只替换语义色。系统字体、原生下拉选择和明确的键盘焦点保留桌面工具的熟悉感。

**Key Characteristics:**

- 墨色中性面与克制蓝色交互。
- 低起伏分组、精确设置行、连续阅读顺序。
- 滑块与可键入数值配对，单位紧邻数值。
- 状态与操作分层，修改不打断当前任务。

## Colors

颜色以界面职责命名；前置 tokens 是规范值，运行时由 `ui/style.css` 的语义 CSS 变量（例如 `--bg`、`--accent`）承载。`dark-*` 为默认主题，`light-*` 为明亮对应值。

### Primary

- **雾蓝交互色**（`dark-accent` / `light-accent`）：主按钮、选中滑轨、开启开关、恢复操作及焦点轮廓。
- **蓝底反差文字**（`dark-on-accent` / `light-on-accent`）：只用于交互色实底上的文字。
- **柔蓝提示底**（`dark-accent-soft` / `light-accent-soft`）：恢复按钮悬停及浏览器预览提示。

### Neutral

- **窗口底色**（`*-bg`）：设置页底层与固定操作栏。
- **分组面色**（`*-panel`）：参数分组与错误提示容器。
- **正文与次级文字**（`*-text` / `*-muted`）：分别承载字段内容及说明、分组标题、状态和单位。
- **细分隔线**（`*-line`）：只分隔同组设置行与底部操作区，不为每个字段增加边框。
- **控件底与悬停底**（`*-input` / `*-hover`）：下拉框、次按钮及数字输入的交互反馈。
- **未选滑轨与滑块**（`*-track` / `*-thumb`）：滑块和开关共用的中性控制色。
- **状态绿与错误色**（`*-success` / `*-danger`）：运行状态点及错误文字，不能取代可读文本。

**The One Accent Rule.** 蓝色承担操作层级；绿色和红色仅表达状态，不用于普通设置分组的装饰。

## Typography

**Display Font / Body Font:** Segoe UI Variable → Segoe UI → Microsoft YaHei UI → sans-serif。没有额外网络字体。

**Character:** 界面以系统无衬线字体现出桌面工具的清晰与熟悉感；层级主要通过字号、字重和留白建立，不靠大面积高饱和颜色。

### Hierarchy

- **Headline**（`headline`）：应用名称，窄窗口在现有断点下缩小至（24px）。
- **Title**（`title`）：分组标题，使用次级文字色，与线性图标并排。
- **Body / Field label**（`body` / `field-label`）：设置名称和基础内容；字段标签使用中等字重。
- **Label**（`label`）：按钮文字；下拉框和数字输入也采用小号控件字，但不强制共用按钮字重。
- **Caption**（`caption`）：字段说明。版本、状态、分组注释与单位另使用现有的小字（11px）。

**The Numeric Pair Rule.** 滑块旁的数字使用等宽数字特性 `tabular-nums`，与可见单位保持一组；不要为了视觉紧凑移除精确输入方式。

## Layout

当前设置页采用单列纵向分组。窗口根布局由头部、可滚动内容、底部操作栏三行组成；只有内容区滚动。具体分组顺序与首屏策略记录在 `.impeccable/surfaces/ui-main-ts.md`，不把该页面策略推广为所有未来界面的限制。

常规内容左右留白为（36px）。分组间距为（27px），同组设置行最低高度为（79px），行内左右留白为（21px），上下留白为（17px）。文字区域可伸缩，滑块与数字区域在宽布局中保持稳定宽度（245px）。

现有断点为（650px）和（490px）。前者收紧页边距、控件区和行间距；后者把滑块行改为上下结构，保留说明、滑块、数值与单位的顺序。界面最低宽度为（360px）；不要把桌面密度误当作无限压缩的理由。

## Elevation & Depth

深度以窗口底、分组面和控件底的色调差异建立。静态分组没有阴影；细线只承担组内分隔。阴影仅见于滑块、开关拇指和浮动错误提示，不能给每个分组增加悬浮卡片感。

### Shadow Vocabulary

- **滑块拇指**（`0 1px 5px #0004`）：帮助识别可拖动手柄。
- **开关拇指**（`0 1px 3px #0002`）：区分开关轨道与活动手柄。
- **错误提示**（`0 6px 28px #0002`）：把浮动消息与参数内容分开。

**The Tonal Depth Rule.** 先用语义面色建立层级；不向静态设置容器追加投影或玻璃材质。

## Shapes

分组使用舒缓的圆角（`rounded.section`），按钮和输入框采用更紧的圆角，避免所有控件共享一个夸大的轮廓。开关是水平胶囊，滑块拇指与状态点为圆形。线性图标保持统一描边，不以图标替代标签。

应用图标采用 `assets/fishcreen.png`，以暗色竖屏与蓝色亮屏组成独立标识。页头按（44 × 44px）完整容纳，不裁切；窄窗口下缩小为（34 × 34px），不把栅格图中的渐变或立体高光扩散为界面组件材质。

## Components

### Buttons

操作层级明确，而不是三枚等权按钮。

- **Primary:** 蓝色实底与反差文字，用于保存；基础最小高度（35px）。
- **Secondary:** 中性控件底，用于取消和应用，悬停改用悬停底色。
- **Text:** 蓝色文字与可选线性图标，用于立即恢复；悬停显示柔蓝底。
- **Hover / Focus:** 颜色过渡为（120ms ease-out）；主按钮悬停提高明度。键盘焦点为交互色轮廓（2px），偏移（4px）。
- **Disabled:** 降低不透明度（0.45），不执行悬停反馈。保存与应用在提交期间禁用；未修改时应用禁用。

### Cards / Containers

它们是连续参数分组，不是可点击入口。

- **Shape / Background:** 分组圆角与分组面色，裁切子内容，静态无阴影。
- **Rows:** 设置名称与说明位于左侧，控件位于右侧；相邻行之间使用一条分隔线。
- **Notes:** 与对应分组紧邻，不将 DDC 响应限制包装成性能承诺。

### Inputs / Fields

原生语义优先，既能拖动也能精确键入。

- **Select:** 原生下拉选择，控件底色、正文色、适度圆角；屏幕与模式的值允许省略显示，完整来源仍保留在选项中。
- **Slider + number:** 蓝色已选轨道、中性未选轨道；数字右对齐，单位独立可读。范围、步长和说明来自字段定义，不在设计文档另设一套参数。
- **Switch:** 使用真实 checkbox 和可点击标签，开启时轨道转为交互色；拇指移动过渡为（140ms ease-out）。
- **Disabled:** 相关滑块控件区域降低不透明度（0.4），同时禁用滑块和数字输入，标签与说明保留。
- **Focus:** 使用可见轮廓；滑块外偏移（8px），开关轮廓落在可见轨道而非隐藏 checkbox 上。

### Status / Feedback

运行状态与保存状态分别呈现。错误使用 `role="alert"` 的浮动消息，包含可读错误原因；正常状态使用状态语义和礼貌播报。浏览器预览明确标为示例数据，不暗示已控制真实显示器。

### Application Mark

图标与名称组成一个水平单元，常规间距为（11px），窄窗口间距为（7px）。图标重复旁边的应用名称，现有实现使用空 `alt` 避免重复播报；品牌资产不代替有意义的控件标签。

### Motion

只为控件状态变化提供短过渡，不增加页面入场或滚动演出。现有 `prefers-reduced-motion: reduce` 会关闭动画和过渡；任何新增反馈都必须保留这一行为。

## Do's and Don'ts

### Do:

- **Do** 保留黑暗默认主题及明亮、跟随系统选项，使用语义色成对维护主题。
- **Do** 让字段名称、说明、控件、数值和单位形成完整的可读操作组。
- **Do** 保留真实标签、键盘焦点、禁用语义和减少动态效果支持。
- **Do** 在只读预览、未连接屏幕、未应用修改和提交失败时准确呈现当前状态。

### Don't:

- **Don't** 将蓝色、绿色和错误色混用为无语义装饰。
- **Don't** 为静态参数分组加入玻璃、渐变装饰或大面积阴影。
- **Don't** 隐藏精确数字输入，或把单位只放进不可见提示中。
- **Don't** 把配置时长描述为硬件保证，也不要把预览误称为真实设备验证。
