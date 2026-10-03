# MetaCubeXD v1.273.1 源码审计与实施记录

> **历史归档**：本文保留当时的规划、发现和验证范围；正文中的“尚未修复/测试”等结论属于历史状态。当前状态见[项目状态](../reference/project_status_zh.md)，后续修复与实机证据见[2026-10-02 验收报告](../testing/reports/vm_acceptance_20261002_zh.md)。

审计日期：2026-09-30。范围：固定版本的核心 API、配置行为、agent 接入和发布资产；真实浏览器/核心联调尚未进行。

2026-10-01 更新：深入审查发现的事务、缓存和生命周期缺口已完成开发并补模拟回归；当前交付范围可开始人工验收，实现及限制见[完成度记录](completion_audit_zh.md)。

## 固定基线

| 项目 | 记录 |
|---|---|
| Release | [v1.273.1](https://github.com/MetaCubeX/metacubexd/releases/tag/v1.273.1) |
| 完整提交 | `8bbc8f58fef71148a94fb5c0ff808f79b057337d`（GitHub tag API 返回 commit） |
| 面板资产 | `releases/download/v1.273.1/compressed-dist.tgz` |
| 实测 SHA-256 | `a178e00b67acabcda2dcef00afa90be6a7bb261e466a67dad58c8478d9553603`，与 release 一致 |
| 资产结构 | 根目录有 `index.html`、`config.js`、`_nuxt/` 和 PWA 资源 |
| 默认接入 | Mihomo TUN/无 TUN 覆盖配置均使用固定 release |
| 实际运行版本 | 本轮未启动真实面板/核心，待记录 |

源码已按完整提交下载并审阅。Agent 的 `AGENT_VERSION` 是 `0.0.0`，接入时须结合 release 提交、协议与 feature 列表判断兼容。

## 核心能力对照

依据：[useApi.ts](https://github.com/MetaCubeX/metacubexd/blob/8bbc8f58fef71148a94fb5c0ff808f79b057337d/packages/ui/composables/useApi.ts)、[useQueries.ts](https://github.com/MetaCubeX/metacubexd/blob/8bbc8f58fef71148a94fb5c0ff808f79b057337d/packages/ui/composables/useQueries.ts)。下表是上游实际调用范围；本项目仅支持 Mihomo，接口能力需按 Mihomo 版本实测。

| 功能 | MetaCubeXD 接口/行为 | ClashTui 现状/下一步 |
|---|---|---|
| 版本与身份 | GET `/version`，区分认证与网络失败 | 已实现请求会话与身份验证 |
| 节点与组 | GET `/proxies`；PUT 节点选择；节点/组测速 | 已有选择、测速及刷新 |
| 代理源 | GET/PUT `/providers/proxies[/name]`；provider/node healthcheck | 已实现核心 provider 页面、更新和健康检查 |
| 规则 | GET `/rules`；PATCH `/rules/disable` | 已实现列表、搜索、详情和支持字段时停用 |
| 规则源 | GET `/providers/rules`；PUT 单规则源 | 已实现状态、更新和回读 |
| 连接 | WebSocket 连接流；DELETE 单连接/全部连接 | 已实现全字段详情、过滤导出、固定集合关闭 |
| 配置 | GET/PATCH `/configs`；PUT 重载 | 已实现持续刷新、临时修改、显式持久化与回读 |
| 缓存与 GEO | POST fake-IP、DNS 清理和 GEO 更新 | Mihomo 已有，需核实版本能力 |
| 更新和重启 | POST `/upgrade`、`/upgrade/ui`、`/restart` | 已实现核心重启/升级入口；面板使用固定包部署，避免漂移版本 |
| 指标与日志 | WebSocket traffic/memory/connections/logs | 已实现总览有界采样、内存流与日志恢复导出 |

### 业务差异

1. [useGeneralConfig.ts](https://github.com/MetaCubeX/metacubexd/blob/8bbc8f58fef71148a94fb5c0ff808f79b057337d/packages/ui/composables/useGeneralConfig.ts) 的模式修改成功回调由配置页用于关闭连接。TUI 提供显式连接处理选项，默认保留连接；该副作用不进入通用请求层。
2. `useUpdateConfigMutation` 先 PATCH 核心；有 agent `config-sections` 能力时再调用控制接口持久化（restart=false）。持久化失败可能留下已成功的运行修改。纯面板没有该持久化步骤。
3. 上游查询缓存按端点隔离，修改成功后使相关查询失效。TUI 请求与回调已按会话代号隔离。
4. 规则停用已使用核心数字索引实现，缺少 disabled 字段时不执行。

## Agent 协议边界

依据：[useControlApi.ts](https://github.com/MetaCubeX/metacubexd/blob/8bbc8f58fef71148a94fb5c0ff808f79b057337d/packages/ui/composables/useControlApi.ts)、[agent/http.ts](https://github.com/MetaCubeX/metacubexd/blob/8bbc8f58fef71148a94fb5c0ff808f79b057337d/packages/agent/src/http.ts)、[agent/index.ts](https://github.com/MetaCubeX/metacubexd/blob/8bbc8f58fef71148a94fb5c0ff808f79b057337d/packages/agent/src/index.ts)。

- 控制路径默认是面板同源 `/api/control`；桌面 preload 可提供另一 base/token。仅通过 Mihomo `/ui` 提供静态资源不会自动提供管理路由。
- 使用独立 Bearer token；health/info 是公开探测接口，features 决定能力。事件流也接受查询 token，不能写入普通日志。
- 内核支持 status/start/stop/restart/rollback/recover，另有内核版本、TUN、系统代理、GEO 和日志接口。
- Profile 支持 CRUD、复制、导入、刷新、验证、激活、刷新并激活和编辑快照/预览/保存。类型是 local/remote/merge/script，与 ClashTui File/URL/Template 需要明确转换。
- 编辑器有 revision 与冲突机制；共享管理层应复用或明确适配。
- 还有 config runtime/section、WebDAV backup/restore，应纳入最终能力矩阵。
- Supervisor 面向 Mihomo；与 ClashTui 模板工作流仍需明确适配。

## 最终实现与核心差异

通用请求使用不可变端点与认证快照；读请求的身份检查缓存两秒，写操作重新检查。端点或认证变化使旧任务失效，旧回调不能污染新会话。非 2xx 统一报错，正文有界且去除当前 secret。活跃页面有间隔和单任务限制，404/405 不被缓存成永久不支持。

连接过滤作用于权威可见行，关闭前固定 ID 集合并报告部分失败。未知元数据保留在详情和导出。日志和采样缓冲有界，WebSocket 使用认证头和系统 TLS 根证书。配置文件校验后原子替换；激活失败恢复旧文件并尝试恢复核心，失败时明确报告。



管理 Web 页面共享本地 Profile、模板、覆盖配置和平台服务函数。数据库写入使用跨进程文件锁，编辑使用内容修订；后台任务恢复已完成/失败结果，中断结果提示核实，不自动重放。没有实现上游 agent 的 script/merge 或 WebDAV 协议；具体交付边界见[功能矩阵](tui_metacubexd_roadmap_zh.md)。

CLI 更新资产选择核对了 [Mihomo 官方 v1.19.32 资产列表](https://github.com/MetaCubeX/mihomo/releases/expanded_assets/v1.19.32)：同平台存在 CPU/Go 变体和系统安装包。更新入口选择保守二进制变体并要求 SHA-256；没有把检查发布元数据当成实际升级测试。

## 验证边界

ClashTui 原始提交为 `3637c2575714cfe20a4b4f9d13de3ed8035acd46`，开发改动尚未提交。发布包已读取消验哈希和资源结构，没有实际部署到核心目录。

本轮执行 Rust 编译、单元和模拟 HTTP/WebSocket 测试、严格 Clippy、格式化与差异检查；既有无默认特性构建问题已修复。Chromium 管理页已使用模拟核心联调；实际安装、真实核心联调与 Windows/macOS 服务测试尚未进行，不能用模拟结果代替。人工步骤见[测试指南](../testing/tui_web_testing_zh.md)。

最终开发检查（2026-10-01）：

| 检查 | 结果 |
|---|---|
| 全功能单元及模拟接口测试 | 350 通过，0 失败，1 项原有手工测试忽略 |
| 隔离进程管理流程与 API 回归 | 6 通过；服务/核心使用桩程序与模拟 HTTP |
| 全目标、全功能 Clippy，`-D warnings` | 通过 |
| 无默认特性 CLI 编译 | 通过；仍有未使用代码警告 |
| Rust 格式化检查 | 通过 |
| 管理页模拟 DOM/fetch 回归 | 5 DOM 测试及 3 Chromium 模拟核心交互测试通过 |
| `git diff --check` | 通过 |
