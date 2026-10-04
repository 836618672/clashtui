# 文档入口

日常使用和测试从下面的入口开始。`archive/` 保留旧规划与历次审查，只有追溯问题时需要阅读。

## 先看哪篇

| 你想做什么 | 阅读入口 |
|---|---|
| 判断项目目前完成到哪里、哪些还没验收 | [当前项目状态](reference/project_status_zh.md) |
| 使用 TUI、CLI，了解配置与订阅 | [使用指南](guides/getting_started_zh.md) · [English](guides/getting_started_en.md) |
| 手动准备 Mihomo 与 ClashTui 配置 | [手动配置](guides/install_manually_zh.md) · [English](guides/install_manually_en.md) |
| 在当前 mini PC 上完成测试、确定何时需要 Mac | [自动闭环测试方案](testing/README.md)，人工只按需做 H01–H06；[简化记录](testing/manual_test_record_zh.md) |
| 重跑自动测试，或管理隔离 VM | [自动测试流水线](testing/test_pipeline_zh.md) · [VM 操作](testing/local_vm_testing_zh.md) |
| 查看最新自动/实机结果 | [Vue 面板验收与后续回归](testing/reports/vue_dashboard_acceptance_20261003_zh.md)；历史报告按日期保留，见[测试入口](testing/README.md) |
| 对照 CLI、TUI、Web 的业务入口与差异 | [三端功能矩阵](reference/cli_tui_web_parity_zh.md) |
| 参与开发 | [功能设计](development/clashtui_feature_design_zh.md) → [代码架构](development/architecture_zh.md) → [Vue 面板](development/web_dashboard_zh.md) → [开发约定](development/development_conventions.md) |
| 了解移除 sing-box 后旧数据如何处理 | [Mihomo 单核心迁移](reference/mihomo_only_migration_zh.md) |
| 追溯设计决策、面板版本锁定与修复过程 | [历史归档目录](archive/README.md) |

建议你现在按 **项目状态 → 自动闭环测试方案** 阅读；Mac 只在人工体验阶段介入，不必逐篇阅读或手工完成 M01–M30。

## 目录与文档状态

| 目录 | 用途 | 如何使用 |
|---|---|---|
| `guides/` | 当前用户指南，中英文各一份 | 操作和配置参考 |
| `development/` | 当前功能设计、架构和开发约定 | 修改代码前阅读；[English design](development/clashtui_feature_design_en.md) / [architecture](development/architecture_en.md) |
| `reference/` | 当前状态、三端矩阵、迁移行为 | 查功能范围与已知边界 |
| `testing/` | 自动测试、VM 和手工测试步骤、记录模板 | 指导执行，不能当作通过证据 |
| `testing/reports/` | 按日期保存的验收结果 | 仅证明该次构建与环境的已执行范围 |
| `archive/` | 已被后续实现或验收取代的过程记录 | 追溯原因；不用于判断当前是否存在缺陷 |

“当前指南”表示维护用途，不表示其中所有平台都已验收。中英文指南是同一主题的两种语言版本；完整三端和测试细节目前以中文文档为主。

## 后续维护规则

- 功能或操作变化时更新对应指南、功能矩阵和项目状态，避免新增另一份“最新完成度”文档。
- 测试步骤只写在 `testing/`；实际执行结果另存 `testing/reports/`，注明日期、构建、环境、通过/失败/跳过及未测范围。
- 新的审查过程记录追加到 `archive/README.md`，说明记录日期与后续状态；未关闭的问题同时登记在项目状态中。
- 旧报告不随新测试改写成“最新通过”。原始日志在 `target/`，通常不随仓库分发；报告保留路径和可重跑入口。
- 移动文档时同步仓库引用和相对链接。编辑器交换文件不属于文档，不纳入导航，也不随整理删除。
