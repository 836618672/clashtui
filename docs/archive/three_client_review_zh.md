# Mihomo 三端功能与对齐复审

> **历史归档**：本文保留当时的规划、发现和验证范围；正文中的“尚未修复/测试”等结论属于历史状态。当前状态见[项目状态](../reference/project_status_zh.md)，后续修复与实机证据见[2026-10-02 验收报告](../testing/reports/vm_acceptance_20261002_zh.md)。

> 后续审查发现的三个边界问题已修复，并增加非法 YAML、慢响应和完整测速 URL 的回归；最新结果见[再次审查记录](three_client_followup_review_zh.md)。下文保留此前七项修复的记录。

审查日期：2026-10-01。初次审查结论：主要业务已有三端入口，但存在下文七项缺口。

## 本轮修复

七项代码/文档问题已处理：TUI 独立启动和重启；共享服务操作验证 Mihomo 身份及配置就绪；CLI `core unfix GROUP` 和 TUI Proxies `u` 恢复自动选择；CLI `core delay NODE --provider PROVIDER`、TUI Providers `d` 及带 Provider 信息的节点测速使用精确端点；TUI 模板生成可输入名称并确认覆盖；三端配置 Test/Check 保留退出状态及 stdout/stderr；新模板示例改为先读取 revision。

测试增加服务未就绪和诊断失败路径、TUI 实际修改操作、Web 模板生成和覆盖取消，以及经 SHA-256 校验的 MetaCubeXD v1.273.1 页面恢复自动选组操作与 CLI 回读。测试只使用临时数据和模拟核心；真实核心及平台服务验收仍待隔离 VM。

最终完整流水线报告：`target/test-results/1790863588469-7d11fe54/report.json`，9 个阶段通过、0 失败；全特性 Rust 350 单元测试及 6 进程测试、Web DOM 5 项、CLI 4 项、TUI PTY 2 项、浏览器 5 项均通过。CLI 特性另外验证 143 单元测试及相同 6 进程测试，覆盖有重叠；既有 1 项手工测试忽略，实机阶段未执行。TUI 模板生成也已接入共享 management 事务和数据库修订检查。

下文保留初次审查的证据，描述的是修复前行为。

Web 范围为固定 MetaCubeXD v1.273.1 纯面板加 ClashTui 本地管理页。审查依据是当前 Rust/HTML 实现、已有测试源码，以及本机缓存的 MetaCubeXD 源码（package.json 标记 1.273.1）；没有重新查询最新上游。没有实际安装、启动代理核心或修改宿主网络。

## 需要补齐的项目

### P1：TUI 的启动入口实际执行重启

`src/tui/tab/srvctl.rs:67` 将 Restart 显示为 “Start Service”，在动作处理中调用 `restart_service()`。没有独立的 Start 动作。CLI 和管理页分别提供 start/restart。

影响：对已经运行的服务点击“启动”会重启并可能中断连接；TUI 也没有语义明确的服务重启入口。应拆分启动和重启，分别调用相同的共享函数，并用服务桩核对实际命令。

### P1：启动/重启成功不保证核心就绪

`src/functions/command.rs:217`、`:231` 直接返回平台服务命令结果；只有 Profile 激活路径调用 API 就绪等待。TUI 成功回调还直接把状态设置为 active。

隔离复现：将临时 systemctl 桩设为成功，模拟 `/version` 和 `/configs` 返回 503，运行 `service start` 仍成功退出，且没有发送核心 API 请求。

影响：此前完成度文档中“启动/重启后检查身份和配置读取”的声明与当前代码不符。应为三端共用的启动/重启增加有界就绪验证，并区分“服务命令已接受”和“核心可用”。

### P2：缺少恢复自动选组能力

缓存上游 `packages/ui/composables/useApi.ts:376` 的 `unfixProxyInGroupAPI` 使用 `DELETE /proxies/{group}`；代理页面对存在 fixed 的组提供“恢复自动选择”。当前 Rust 代理接口和 CLI/TUI 只有选择与测速，没有对应解除固定操作。

影响：在 Web 固定自动组节点后，无法通过 TUI/CLI 执行同样的恢复操作。应补共享 API、能力判断、CLI 命令和 TUI 动作。

### P2：缺少 Provider 范围内单节点测速

缓存上游 `useApi.ts:386` 根据 Provider 使用 `/providers/proxies/{provider}/{node}/healthcheck`。Rust 只有全局 `/proxies/{node}/delay`、组测速及整个 Provider 健康检查，CLI 也没有 Provider 参数。

影响：Provider 中未进入全局代理映射的节点，或不同 Provider 的同名节点，无法由 TUI/CLI 精确测速。整组健康检查不能替代指定节点的结果。

### P2：模板生成的命名与覆盖策略不同

`src/tui/tab/files/template.rs:390` 固定使用 `{模板文件名}.tpl` 作为 Profile 名称，直接生成；同模板重生成允许覆盖，没有额外确认。CLI/Web 可指定生成名称，覆盖已有生成配置分别需要 `--yes` 和页面确认。

影响：TUI 无法完成与 CLI/Web 相同的“同模板生成多个不同名称配置”工作流，且覆盖保护不同。应统一命名输入、冲突判断和覆盖确认。模板核心校验及写入回滚已经存在，不属于此处缺口。

### P2：Test 与 Check 的结果语义不同

TUI Test 使用 `test_config()` 展示核心 stdout/stderr；Check 使用 `check_config()` 返回有效性。管理层将 `test` 和 `check` 合并，只返回有效性，成功时不保留核心测试输出。因此 CLI/Web 的“测试”无法查看 TUI 同等诊断信息。

应统一返回退出状态、stdout/stderr 和校验结果，再由三端选择展示形式。

### P2：新模板命令示例失效

`docs/reference/cli_tui_web_parity_zh.md` 使用 MD5 空内容值 `d41d8cd98f00b204e9800998ecf8427e`；实现 `management::revision()` 使用 SHA-256。按示例在隔离目录执行，返回 `Revision conflict: reload the document before saving`。

建议示例先执行 `manage read --kind template --name custom.yaml`，再使用其返回的 revision，避免再次硬编码旧算法。

## 已具备的主要能力

节点选择与普通测速、规则及 Provider 查询/更新、连接查询/关闭、日志、指标、运行设置、Profile 导入/更新/激活、覆盖配置、模板、服务和固定面板部署均能找到相应入口。CLI/Web 大量复用 management；TUI 部分复用 management，部分直接调用更底层函数。因此“共用 Rust 代码”不能自动证明每个操作的确认、返回结果和失败处理一致。

TUI 外部编辑器与 CLI/Web 修订保存的并发保障差异、CLI 有限采样与界面持续展示的差异，属于需要明确记录的交互边界。Windows 系统代理和服务注册按平台提供。上游桌面壳及 agent 的 WebDAV/script/merge 仍属于原先排除的范围。

## 测试证据和缺口

- 上次完整流水线：9 阶段通过，350 项全特性单元测试、6 项隔离进程测试；CLI 特性测试与其有重叠。此次未重复运行整套流水线。
- 此次额外执行上述两项定向隔离复现：旧模板 revision 被拒绝、API 不可用时服务启动仍成功。
- 现有 Chromium 三个场景只覆盖本地管理页的认证、导入/保存/导出、冲突、取消和运行设置；没有加载 MetaCubeXD 页面。
- 现有 TUI PTY 只验证启动、九标签导航、帮助和退出，不执行修改操作。
- 缺少逐项“三端操作 → 另两端读取”的一致性回归，特别是模板生成、服务生命周期、节点解除固定及 Provider 单节点测速。
- 实际 Mihomo、锁定面板资源、Windows/macOS 服务和代理网络仍需隔离 VM 验收。

建议顺序：先修服务语义与就绪检查，再补两个核心 API 能力，统一模板和测试结果，修正文档示例；最后增加三端共享场景与锁定面板浏览器回归，再进行隔离 VM 验收。
