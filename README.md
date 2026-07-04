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

## Configuration / 配置

Open the tray menu settings and fill in:

在托盘菜单的设置中填写：

- Model URL ending with `/v1` / 以 `/v1` 结尾的模型 URL
- API Key
- Model name / 模型名
- UI language / 界面语言
