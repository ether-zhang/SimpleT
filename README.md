# SimpleT

SimpleT is a lightweight desktop translation tray app built with Tauri. It calls an OpenAI-compatible chat completions API and keeps a small flyout UI near the system tray.

SimpleT 是一个轻量级桌面托盘翻译工具，基于 Tauri 构建。它调用兼容 OpenAI Chat Completions 的接口，并在系统托盘附近显示简洁的悬浮翻译窗口。

## Features / 功能

- Tray-based quick translation flyout / 托盘快速唤起翻译窗口
- OpenAI-compatible endpoint, API key, and model settings / 支持配置兼容 OpenAI 的接口、API Key 和模型名
- Automatic model discovery with connection feedback and a searchable, height-limited dropdown / 自动获取模型列表，显示连接状态，支持搜索及限高滚动选择
- Bidirectional language swap / 支持源语言和目标语言互换
- Localized UI language selection / 支持界面语言切换
- Appearance switching between Default and Liquid Glass, applied immediately and remembered / 外观支持默认与液态玻璃，即时切换并记住选择
- Local translation history: newest at the bottom, scroll upward for older entries / 本地翻译历史：最新记录在底部，向上滚动查看更早的记录
- Persistent flyout: click the tray icon or × to hide it; clicking elsewhere keeps it open / 浮窗保持显示，点击托盘图标或 × 收起，点击其他位置不会收起

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

Tauri 2.12.1 or later is required. The lockfile includes the `tray-icon` fix for macOS 27 swallowing left clicks when a menu is attached ([upstream fix](https://github.com/tauri-apps/tray-icon/pull/365)). Left click toggles the translation panel; right click opens the menu.

要求 Tauri 2.12.1 或更新版本。锁定的依赖包含 macOS 27 左键点击被托盘菜单拦截的修复。左键切换浮窗显示和收起，右键打开菜单。

Before a macOS release, verify on a real Mac: left/right tray clicks, immediate typing and Chinese IME, Escape and ×, staying visible after outside clicks, reopening, multiple displays with different scaling, and full-screen Spaces. Windows tests cannot validate these AppKit behaviors.

发布 macOS 版本前，请在真机验证托盘左右键、打开后输入、中文输入法、Esc 和 ×、点击外部后保持显示、重新打开，以及不同缩放的多屏和全屏空间。这些 AppKit 行为无法通过 Windows 测试验证。

Also check opening Settings immediately after launch, repeated tray clicks during the close animation, editing or clearing an API key while saving, and changing the tray menu language without restarting. Edits made during a save remain as unsaved drafts.

同时检查启动后立即打开设置、收起动画中重复点击托盘不会重新拉起、保存期间编辑或清除 API Key，以及无需重启的托盘语言更新。保存期间新增的编辑会保留为未保存草稿。

## Configuration / 配置

Settings includes an Appearance menu with Default and Liquid Glass. Changing appearance saves only this preference, so unfinished API or model edits remain drafts. Liquid Glass combines a borderless native frosted backdrop (Popover on macOS, Acrylic on Windows) with shared glass highlights and surfaces in the WebView. Background tint is masked out beneath controls, which retain their own tint. On macOS, control regions also use separate native frost opacity from the translucent backdrop in blank areas. Overlapping controls share one opacity group. Scrolling and dropdowns update these regions without replacing the material. Material is prepared before showing the window and retained between openings, with one native mask revealing the backdrop and HTML together. Default removes the material and restores the opaque appearance. See [Tauri window effects](https://v2.tauri.app/reference/javascript/api/namespacewindow/#effect) for platform support.

设置中的“外观”提供“默认”和“液态玻璃”。切换会立即应用并单独保存，不会连带保存尚未完成的 API 或模型配置。液态玻璃由无描边的原生磨砂底层和统一的玻璃控件样式组成，底层在 macOS 使用 Popover、Windows 使用 Acrylic。空白底色会避开控件区域，控件使用独立底色；macOS 的控件磨砂覆盖度与空白底层分别设置，重叠控件统一合成透明度，滚动和展开下拉菜单只更新覆盖范围。材质在窗口显示前准备好，关闭后保留供下次使用，并用同一个原生遮罩展开磨砂底层和网页内容。切回默认会移除原生材质并恢复实色界面。

Successful translations are saved automatically to `history.json` in the application's local data directory. Use the history button below Settings to browse the original text, translation, languages, and time. History is loaded in batches as you scroll upward and is retained across restarts. Failed requests are not recorded. An unreadable history file is preserved and reported in the history page.

成功的翻译会自动保存到应用本地数据目录下的 `history.json`。点击设置按钮下方的历史按钮，可以查看原文、译文、语言和时间。向上滚动时分批加载旧记录，重启后仍然保留；失败的请求不记入历史。历史文件无法读取时会保留原文件，并在历史页显示错误。

Open the tray menu settings and fill in:

在托盘菜单的设置中填写：

- Model URL ending with `/v1` / 以 `/v1` 结尾的模型 URL
- API Key
- Model from the service's available-model dropdown / 从服务返回的下拉列表中选择模型
- UI language / 界面语言

After entering the API base URL and, when required, an API key, SimpleT requests `GET <base URL>/models`. The URL field shows whether the service returned a usable model list. Selection stays disabled while loading or after a connection, authentication, or response error; use Retry to check again. Long catalogs scroll in a compact dropdown and can be searched by the full model ID. Long IDs are shortened visually, with the full ID available on hover. A saved model remains selected when it is still listed; otherwise choose a replacement explicitly. Discovery does not save settings. This uses the model-list format documented by [OpenAI](https://developers.openai.com/api/reference/resources/models/methods/list) and [OpenRouter](https://openrouter.ai/docs/api/api-reference/models/get-models).

填写 API 基础 URL 及服务所需的 API Key 后，SimpleT 会请求 `GET <基础 URL>/models`，在 URL 下方显示连接及模型获取状态。加载中、连接或认证失败、响应无效时，模型选择保持禁用，可点击“重试”重新获取。下拉列表限制高度并支持按完整模型 ID 搜索；过长名称显示省略号，悬停可查看完整内容。已保存的模型仍在列表中时会保留，否则需要重新选择。获取列表不会自动保存设置。已保存的 Key 仅用于原来的 API 地址，改用其他服务时应填写对应的 Key。
