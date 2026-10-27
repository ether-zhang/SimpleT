# Native 编辑快捷键回归

只针对 `native-macos`。下面的原生菜单检查不等于真实键盘行为验证，尤其不能证明非激活 NSPanel 的分发路径已经修复。

## 在真实 macOS 上构建与检查

在项目根目录执行：

```bash
cargo fmt --manifest-path native-macos/Cargo.toml --check
cargo test --manifest-path native-macos/Cargo.toml --locked
npm run native:build
open "native-macos/target/release/bundle/SimpleT.app" --args --fixture
```

`--fixture` 使用独立测试数据，避免改动正式配置和历史。密码测试使用临时测试文本，不使用真实 API Key。

`edit_menu` 测试使用独立的主线程入口（`harness = false`），创建真实 AppKit 菜单，检查安装、对象保活、六个 selector、键值/修饰键及 nil target。非 macOS 会明确输出 SKIPPED。

## 真机键盘检查

| 区域 | 预期 |
| --- | --- |
| 翻译输入区 | 选区复制/剪切、光标位置粘贴、选区替换、全选、连续撤销/重做 |
| 翻译结果区选区 | 可以复制/全选；剪切和粘贴不能改动结果 |
| 设置 URL | 在光标中间插入、选区替换，以及本次编辑中的撤销/重做 |
| API Key 密码框 | 按 AppKit 安全规则验证；禁止复制/剪切不算失败，不通过应用按钮绕过限制 |
| 模型搜索 | 选区复制/剪切、插入/替换、全选、撤销/重做，筛选内容随编辑更新 |

每个可编辑区域分别验证 `⌘C`、`⌘V`、`⌘X`、`⌘A`、`⌘Z`、`⇧⌘Z`。例如输入 `alpha beta gamma`，只选中 `beta` 复制，粘贴到文本中间，再选择另一段文本粘贴替换，连续撤销并重做；检查不会变成复制全文或向末尾追加。

文本框遵循 AppKit 自身的撤销作用域，不要求离开字段后仍保留该字段的撤销记录。只读结果区不能被修改。

还需要逐项验证：

- 鼠标聚焦、切换字段、切换到其他应用再返回；命令必须作用于当前输入框。
- 点击托盘或 × 隐藏，再显示；反复操作不会失去编辑能力。
- 浮窗可见但非 key window 时，不影响其他应用的快捷键。
- 默认外观、Liquid Glass、多屏及全屏 Space。
- 中文输入法组合期间和提交后，选区、粘贴及撤销保持系统语义。
- 原有 `⌘Enter` 翻译和 `Esc` 行为，包括中文组字期间的保护。
- 托盘菜单的翻译、设置、退出及原有浮窗显示/隐藏行为。

## 若补菜单后仍失效

先记录 macOS 版本、焦点区域、应用激活状态、`keyWindow`、`firstResponder` 的类、是否有 marked text，以及 Edit 菜单是否存在/可用。诊断日志不要输出密码框文本或剪贴板内容。

确认面板为 key window、预期文本编辑器为 first responder，仍无法通过标准菜单处理命令后，再评估仅针对该 panel 的最小 `performKeyEquivalent:` 转发。当前实现没有加入该转发，也没有更改激活策略。
