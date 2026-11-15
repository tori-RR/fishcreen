# 开发与验证

当前程序使用 Rust + Tauri；`src/` 与 `build-legacy.ps1` 是保留的旧 C# 实现。

## 环境

使用已配置 Rust MSVC x64、Visual Studio C++ Build Tools、Windows SDK 和 Node.js/npm 的 PowerShell。

```powershell
npm ci
npx playwright install chromium
```

Playwright 仅是开发依赖，不进入应用运行包。也可为本次 PowerShell 会话设置 `FISHCREEN_BROWSER_CHANNEL=chrome` 或 `msedge`，使用已安装的浏览器；测试仍使用临时独立配置。

## 一键验证

```powershell
.\check.ps1
```

入口检查前端依赖指纹，执行 TypeScript、前端状态与参数测试、构建依赖策略测试、Rust 格式与单元测试、Chromium smoke，最后进行正式优化构建。

构建默认输出到 `D:\Apps` 下新建的验证目录，不覆盖或启动已安装的程序。可用 `-NoBuild` 仅运行检查，用 `-OutputDirectory` 指定一个尚不存在的验证目录。

也可分别运行：

```powershell
npm run check
npm test
npm run test:build
npm run test:ui
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml --locked -j 1
```

浏览器入口会自动启动并关闭仅绑定 `127.0.0.1` 的预览和临时 Chromium 实例；每个 suite 独立运行。截图写入忽略的 `output/playwright/` 与 `.impeccable/review/`。

## 验证边界

Node 回归执行真实前端模块，但 DOM 与 IPC 使用模拟对象。Chromium 回归使用真实 DOM 和页面内 IPC fixture，不调用物理 DDC、写启动项或应用真实配置。Rust 的慢设备、取消、恢复文件和日志用例使用模拟时钟或独立临时目录。

这些测试不能代替真实显示器的 DDC、热插拔、睡眠唤醒和混合 DPI 验收。CI 仅检查与构建，不部署、不创建 Release。
