# SimpleT

SimpleT is a lightweight desktop translation tray app built with Tauri. It calls an OpenAI-compatible chat completions API and keeps a small flyout UI near the system tray.

SimpleT 是一个轻量级桌面托盘翻译工具，基于 Tauri 构建。它调用兼容 OpenAI Chat Completions 的接口，并在系统托盘附近显示简洁的悬浮翻译窗口。

## Features / 功能

- Tray-based quick translation flyout / 托盘快速唤起翻译窗口
- OpenAI-compatible endpoint, API key, and model settings / 支持配置兼容 OpenAI 的接口、API Key 和模型名
- Bidirectional language swap / 支持源语言和目标语言互换
- Localized UI language selection / 支持界面语言切换

## Development / 开发

Requirements / 环境要求：

- Node.js
- Rust
- Tauri system dependencies

Install dependencies and run the Tauri dev app:

安装依赖并启动 Tauri 开发环境：

```bash
npm ci
npm run tauri dev
```

Build a release package for the current platform:

构建当前平台的发布包：

```bash
npm run tauri build
```

Run the regression checks:

运行回归检查：

```bash
npm test
cargo test --manifest-path src-tauri/Cargo.toml --locked
cargo fmt --manifest-path src-tauri/Cargo.toml --check
```

GitHub Actions runs these checks on Windows and macOS. A damaged configuration is reported in the UI and is preserved until a complete configuration is explicitly saved in Settings.

GitHub Actions 在 Windows 和 macOS 上运行上述检查。配置损坏时界面会显示错误；在设置中明确保存完整配置之前，自动保存不会覆盖原文件。

## macOS compatibility / macOS 兼容性

Tauri 2.12.1 or later is required. The lockfile includes the `tray-icon` fix for macOS 27 swallowing left clicks when a menu is attached ([upstream fix](https://github.com/tauri-apps/tray-icon/pull/365)). Left click opens the translation panel; right click opens the menu.

要求 Tauri 2.12.1 或更新版本。锁定的依赖包含 macOS 27 左键点击被托盘菜单拦截的修复。左键打开翻译浮窗，右键打开菜单。

Before a macOS release, verify on a real Mac: left/right tray clicks, immediate typing and Chinese IME, Escape, outside-click dismissal, reopening, multiple displays with different scaling, and full-screen Spaces. Windows tests cannot validate these AppKit behaviors.

发布 macOS 版本前，请在真机验证托盘左右键、打开后输入、中文输入法、Esc、点击外部收起、重新打开，以及不同缩放的多屏和全屏空间。这些 AppKit 行为无法通过 Windows 测试验证。

Also check opening Settings immediately after launch, reopening during the close animation, editing or clearing an API key while saving, and changing the tray menu language without restarting. Edits made during a save remain as unsaved drafts.

同时检查启动后立即打开设置、收起动画中重新打开浮窗、保存期间编辑或清除 API Key，以及无需重启的托盘语言更新。保存期间新增的编辑会保留为未保存草稿。

## Configuration / 配置

Open the tray menu settings and fill in:

在托盘菜单的设置中填写：

- Model URL ending with `/v1` / 以 `/v1` 结尾的模型 URL
- API Key
- Model name / 模型名
- UI language / 界面语言
