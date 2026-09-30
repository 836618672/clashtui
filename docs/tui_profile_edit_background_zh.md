# TUI 订阅编辑问题与项目背景

本文记录 Files 页面订阅编辑问题的原因、当前实现和这次 TUI 改进的范围，供后续开发与排查使用。用户操作说明见[使用指南](./getting_started_zh.md#编辑订阅)，代码整体结构见[架构文档](./architecture_zh.md)。

## 项目背景

ClashTui 是 Rust 终端界面程序，管理 Mihomo 与 sing-box 两种代理核心。它通过 CLI 子命令支持脚本操作，也提供 TUI 用于交互式管理。

启动入口在 `src/main.rs`：解析参数后初始化配置；有 CLI 子命令时直接处理，否则初始化主题、按键与终端，再启动 TUI。配置目录由 `--config-dir` 或 `CLASHTUI_CONFIG_DIR` 指定，也可以使用默认的用户配置路径。

配置数据分成几类：

| 数据 | 文件 | 用途 |
|---|---|---|
| 程序与服务配置 | `config.yaml` | 核心可执行文件、配置路径、服务和外部编辑命令 |
| Profile 数据库 | `clashtui.db` | 当前核心、Profile 类型、订阅 URL、选中 Profile 和更新选项；文件内容是 YAML |
| Mihomo 覆盖配置 | `mihomo/core_override_config.yaml` | 选择 Mihomo Profile 时覆写顶层配置项 |
| sing-box 覆盖配置 | `sing-box/core_override_config.json` | 选择 sing-box Profile 时递归合并对象、替换数组 |
| Profile 文件 | 各核心目录下的 `profiles/` | 下载的订阅配置或本地导入配置 |

`ProfileManager` 在 Mihomo 与 sing-box 下分别保存 Profile 列表和当前选择。数据库允许两个核心有同名 Profile，因此查找、改名、删除都必须按当前核心操作。

## TUI 与按键流程

`App::serve()` 每秒约渲染 50 帧，并接收按键、窗口尺寸变化和异步任务完成事件。按键依次经过弹窗、全局组合键、帮助面板、标签页组合键、当前标签页和全局按键。

Files 是 `DualTab<Profile, Template>`：左侧管理订阅，右侧管理模板。两边共享当前焦点，窄终端应优先显示有焦点的面板。Profile 和 Template 分别定义默认按键；`keymap.yaml` 可以按核心、标签页覆盖按键，也支持多键组合。

输入框由弹窗接管按键，直到用户按 Enter 或 Esc。配置文件外部编辑则由 `extra.edit_cmd` 处理，通常用于启动 Vim 或系统编辑器。

## `e` 偶尔不可用的原因

原来的 `e` 绑定其实存在，但执行的是“打开下载后的 YAML / JSON 文件”，不是用户期望的“编辑订阅名称或 URL”。当文件不存在、编辑命令配置错误或编辑器无法启动时，用户会看到 `e` 没有达到预期；一部分命令以分离进程启动，错误反馈也不充分。

按键路由中还有几个会让功能间歇失效的条件：

1. **组合键前缀吞掉普通按键。** 如先按 `g` 进入 `gg` 组合键，再按 `e`，原处理器会把不匹配的 `e` 消耗掉，不再交给单键快捷键处理。自定义三键组合也可能在只匹配到中间前缀时提前执行。
2. **局部按键配置替换了整份默认绑定。** 旧逻辑只要加载到一个自定义绑定，就只查用户配置的映射。例如只改 `j`，默认的 `e`、`i` 等功能也会消失。
3. **筛选行号被当成数据库索引。** 界面先筛选 `items`，但动作仍以可见列表中的行号访问未筛选列表。它可能编辑、更新或删除另一条 Profile；空筛选结果也容易产生无效选中。
4. **同名 Profile 查找没有按核心限定。** 数据库中的 Mihomo 和 sing-box Profile 可以同名，先查 Mihomo 的通用查找逻辑可能返回另一核心的订阅。删除时两种格式的同名缓存也曾一起删除。
5. **筛选和输入交互缺少反馈。** 搜索后光标可能超出可见范围；FZF 返回的是筛选列表索引；预填文本缺少可靠的清空和光标操作；模板预览尚未实现。
6. **随安装提供的按键文件与当前动作枚举有旧名字差异。** 原配置里的 `TrafficNext` / `TrafficPrev` 不再是当前动作名，可能使按键配置加载失败。

这些因素会让“看起来按了按键，却没有执行对应功能”呈现为偶发问题；还可能让某条编辑等操作实际命中错误订阅。

## 当前修改

在 Profile 列表按 `e` 打开名称编辑框；若 Profile 类型是 URL，再打开 URL 编辑框。两个字段都预填当前值。输入无效时提示原因并保留可修正的值；任一环节按 Esc 均取消整个编辑。名称验证会阻止空名称、路径分隔符和平台保留字符，URL 仅接受带主机的 HTTP 或 HTTPS 地址。

确认保存时先构造数据库副本，再写临时数据库文件；改名时同步重命名本地 Profile 文件。持久化失败会尝试恢复原文件名，不覆盖已有目标。当前 Profile 名称变化时同步当前选择，更新选项也随 Profile 保留。改 URL 不会自动联网下载；保存后按 `u` 拉取新地址。

按 `E` 仍表示打开配置文件。普通 File、URL Profile 可编辑本地 YAML；sing-box Profile 可编辑本地 JSON。名称和 URL 的修改在 TUI 内完成，不依赖 `extra.edit_cmd`。

同时已修改按键分发、列表筛选和反馈：

- 默认按键与用户的局部覆盖合并；显式自定义组合键可以覆盖同前缀默认单键。
- 组合键前缀不匹配时恢复到普通按键处理；完整多键组合匹配后才触发动作。
- Profile 与 Template 的动作、FZF 和光标都按可见列表映射；空结果取消选中，Enter 可生成模板。
- Files 面板显示焦点、数量、选中位置和空列表说明；页脚按屏幕宽度展示按键，窄屏只展开当前标签名。
- 加入筛选提示、Esc 清除筛选、编辑成功反馈、直接按键预览模板、预填输入框及 Home / End / Ctrl-U 操作。
- 同名 Profile 的查找、修改、删除按活动核心隔离；导入后选中新 Profile。模板预览不再触发未实现分支。
- 旧 Traffic 快捷键名称可以读取，并映射到当前可用的流量显示动作。

## 改动入口

| 代码 | 相关职责 |
|---|---|
| `src/functions/file/profile.rs` | Profile 名称与 URL 校验、原子保存、缓存文件改名与回滚 |
| `src/config/database.rs` | 活动核心的 Profile 查找、当前项维护和同名隔离 |
| `src/tui/tab/files/profile.rs` | `e` / `E` 行为、导入、筛选操作、可见行和状态反馈 |
| `src/tui/tab/files/template.rs` | 模板焦点、筛选、Enter 生成和预览 |
| `src/tui/tab/files.rs` | Profile/Template 共用可见列表与选中逻辑 |
| `src/tui/widget/chord.rs` | 多键序列消歧和未匹配按键回退 |
| `src/tui/tab/mod.rs`、`src/tui/app.rs` | 默认按键合并、焦点与响应式页签/页脚 |
| `src/tui/popmsg/input.rs` | 预填编辑及光标、删除和清空输入 |

## 验证记录与限制

- `cargo test --locked --all-features --no-fail-fast`：**360 通过、0 失败、1 忽略**。
- `cargo check --locked --all-features`：通过。
- `cargo clippy --locked --all-targets --all-features -- -D warnings`：通过。为修复现有 macOS Clippy 告警，调整了几个条件表达式，并移除了原来恒真的平台测试。
- `cargo fmt --all -- --check`、`git diff --check`：通过。
- 使用临时配置和本机模拟 API，通过伪终端实测 Mihomo 120 列、sing-box 70 列的导入、预填编辑、取消、保存、文件改名、组合键回退、筛选后编辑、空结果恢复和模板预览。

命令行测试运行时曾触发 macOS 测试临时目录同纳秒碰撞；生成目录现改用随机值。config 路径的 macOS 单测也改为从模块作用域引用私有函数。

本机伪终端验证采用短生命周期临时工作目录，模拟数据不会影响真实订阅或服务。编辑器外部进程的行为未包含在该伪终端流程中；`E` 仍按本机 `extra.edit_cmd` 或系统默认程序打开配置文件。
