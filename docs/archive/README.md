# 历史归档

这些文档有追溯价值，保留当时的发现、决定和测试证据。正文中的“当前”“尚未修复”“未进行真实测试”按原记录时间理解；判断今天的状态请回到[项目状态](../reference/project_status_zh.md)。

| 文档 | 保留原因 | 当前替代入口 |
|---|---|---|
| [TUI 与 MetaCubeXD 开发路线](README.md#roadmap) | 原始功能对照、实施阶段与共享管理层决策 | [功能设计](../development/clashtui_feature_design_zh.md)、[三端矩阵](../reference/cli_tui_web_parity_zh.md) |
| [MetaCubeXD v1.273.1 审计](README.md#upstream) | 固定提交、资产摘要、上游 agent/核心 API 边界 | [当前范围与固定版本](../reference/project_status_zh.md) |
| [目标完成度与开发闭环](README.md#completion) | 早期开发缺口及逐项关闭记录 | [项目状态](../reference/project_status_zh.md)、[实机报告](../testing/reports/vm_acceptance_20261002_zh.md) |
| [三端七项复审](README.md#review) | 服务语义、就绪、API、模板和诊断问题与修复依据 | [三端矩阵](../reference/cli_tui_web_parity_zh.md)、[实机报告](../testing/reports/vm_acceptance_20261002_zh.md) |
| [七项修复后的再次审查](README.md#followup) | YAML 诊断、测速预算和 URL 编码三个边界的修复 | [实机报告](../testing/reports/vm_acceptance_20261002_zh.md) |
| [Provider 工作流审查](README.md#providers) | 四项 Provider 缺陷的原始复现与证据 | [五项修复及回归](../testing/reports/vm_acceptance_20261002_zh.md) |
| [TUI 订阅编辑问题与背景](README.md#profile-editor) | 按键、筛选、元数据编辑等历史根因 | [使用指南](../guides/getting_started_zh.md)、[代码架构](../development/architecture_zh.md) |

仅有新的日期报告才能增加验收证据；归档本身不意味着所有平台已经通过。单核心迁移仍对旧用户有用，保留在[当前参考](../reference/mihomo_only_migration_zh.md)中。

## roadmap

**ClashTui 与 MetaCubeXD 开发路线及交付状态**


更新：2026-10-01。MetaCubeXD 固定为 **v1.273.1**，提交 `8bbc8f58fef71148a94fb5c0ff808f79b057337d`。当前交付范围的开发及审查缺口已闭环，可以开始人工验收，见[完成度与证据](README.md#completion)。按用户要求，没有进行实际安装或启动真实核心；Chromium 管理页已完成模拟核心联调。

#### 目标与架构决策

两端管理同一核心，共同操作以核心回读结果为准，本地订阅、模板和服务管理形成互补全集。交付形态是 **ClashTui TUI + 固定版本 MetaCubeXD 纯面板 + ClashTui 本地管理 Web 页面**。

MetaCubeXD 核心面板由核心 `/ui/` 提供；本地管理页面由 `clashtui web` 提供，两者有不同地址和认证。上游纯面板不会自动获得 ClashTui 的本地管理接口。选用独立管理页面，是因为上游 agent 的 Profile 类型、Mihomo supervisor 和本项目本地模板模型不能直接对应。没有实现上游 `/api/control` 的兼容伪装。

全集以本表的代理核心管理和 ClashTui 本地工作流为边界。MetaCubeXD 桌面壳、agent 的 script/merge Profile、WebDAV 备份恢复不在本轮交付范围，不能据此宣称所有部署形态完全等价。

#### 功能矩阵

| 能力 | TUI 交付 | Web 交付 | 同步与限制 |
|---|---|---|---|
| 核心身份、在线状态 | 请求会话、版本检查、Status | MetaCubeXD 核心状态；管理页服务状态 | 认证、网络、身份不匹配分别报告；旧会话结果丢弃 |
| 节点、组、测速 | 原有选择、测速、搜索、排序 | MetaCubeXD | 同一核心 API；页面刷新读取外部修改 |
| 代理 Provider | Providers 页，详情、更新、健康检查 | MetaCubeXD | URL 编码；批量更新报告部分失败 |
| 规则、规则 Provider | Rules / Providers 页，搜索、详情、更新；字段支持时停用规则 | MetaCubeXD | 保留核心规则索引；接口失败不当作成功 |
| 连接 | 全字段详情、过滤、导出、固定 ID 集合关闭 | MetaCubeXD | 批量关闭仅影响确认时可见集合 |
| 指标 | 会话统计、实际间隔速率、连接数、有限曲线、可用内存 | MetaCubeXD 图表 | TUI 会话量不等同核心生命周期累计量 |
| 日志 | 有界缓冲、重连、过滤、暂停、导出 | MetaCubeXD | 本地过滤等级与核心日志等级分开 |
| 运行设置 | Settings 刷新、字段能力判断、临时 JSON 修改、显式持久化 | MetaCubeXD 临时设置；管理页修改和持久保存 | Mihomo 仅开放已知可写且存在字段 |
| 模式变化后的连接 | 默认保留；用户可选择关闭连接 | 上游模式操作具有关闭连接行为 | 副作用是显式策略，不放入通用请求层 |
| 配置与订阅 | Files / CLI 导入、更新、编辑、激活、删除 | 管理页导入、URL 新建、编辑、更新、激活、重命名、删除 | 同一数据库和文件；后台写入跨进程锁；Web 编辑有内容修订校验 |
| 模板、覆盖配置 | 模板生成、编辑、覆盖合并 | 管理页生成、新建/编辑模板、编辑覆盖配置、导出 | 使用 Mihomo 目录；重新进入或活跃轮询发现外部修改 |
| 配置激活 | 校验、原子替换、重载、回读、失败恢复 | 共用激活入口 | 失败恢复也会回读；恢复失败明确报告；远程端点不激活本机文件 |
| 服务生命周期 | 平台服务启动、停止、重启 | 管理页启动、停止、重启 | 三端共用 Mihomo 平台函数 |
| Windows 系统代理/服务注册 | 平台操作 | 管理页按平台显示 | 其他平台隐藏；尚未进行 Windows 实机验证 |
| 核心维护 | Mihomo API 重启/升级及已有缓存/GEO 操作 | MetaCubeXD | 核心接受请求后需核实实际版本，不声称升级必定成功 |
| 面板部署 | Status 打开/部署；CLI panel | 管理页打开/部署 | 固定发布包 SHA-256；Mihomo 使用校验后的归档部署 |
| 远程端点 | 配置保存 Mihomo 端点/认证 | MetaCubeXD 自身端点管理 | 远程核心不能控制本机服务 |

模板订阅源与核心运行 Provider 是不同资源，更新一方不自动代表另一方已更新。可选字段缺失表示未知或不可用。

#### 实施阶段结果

| 阶段 | 开发结果 | 后续验收 |
|---|---|---|
| P0 基线 | 固定版本、提交、资产哈希；核心/agent 源码审计 | 记录实际安装的核心版本和面板版本 |
| P1 请求与同步 | 端点/认证快照、会话代号、身份缓存、错误分类、活跃页单任务轮询 | 双端修改可见、认证失败、端点变化与断线 |
| P2 核心功能 | 规则、Provider、连接诊断、指标、日志导出 | Mihomo 逐接口验证，记录版本差异 |
| P3 本地工作流 | 临时/持久设置、校验激活与恢复、面板入口、远程操作边界 | 持久保存、重启、校验失败和恢复失败 |
| P4 共享管理 | 独立 Web 页面、Bearer 认证、后台任务、数据库锁、文档修订 | 两端文件同步、并发编辑、任务期间重启 |
| P5 发布 | 使用说明和配对测试清单已编写 | 真实浏览器、核心及平台服务验收；验收后整理发布 |

共享层通过业务函数和受锁保护的本地存储复用，TUI 不需要调用本地 HTTP 服务才能工作。管理服务保存最新任务结果，重启恢复已完成/失败结果；中断任务提示核实，不自动重放。手工编辑器不参与锁协议，应关闭后再更新或激活。

#### 检查和测试顺序

开发检查：格式化、全功能编译、单元与模拟 HTTP/WebSocket 测试、严格 Clippy、无默认特性 CLI 编译、Web JavaScript 语法与差异空白检查。

真实安装和服务测试按用户要求推迟到开发完成后。下一步执行[双端测试指南](../testing/tui_web_testing_zh.md)，不要把模拟测试结果视为真实核心或跨平台兼容结论。上游证据与核心限制见[审计记录](README.md#upstream)。

后续核对补齐了 Web 的批量更新、配额、订阅 URL 元数据、独立配置校验、模板删除/Provider 分组/生成预览和服务管理；CLI 新增 `manage`/`core` 命令，TUI 新增模板创建和生成预览。逐项对应入口及有意保留的交互差异见[CLI/TUI/Web 对齐记录](../reference/cli_tui_web_parity_zh.md)。

## upstream

**MetaCubeXD v1.273.1 源码审计与实施记录**


审计日期：2026-09-30。范围：固定版本的核心 API、配置行为、agent 接入和发布资产；真实浏览器/核心联调尚未进行。

2026-10-01 更新：深入审查发现的事务、缓存和生命周期缺口已完成开发并补模拟回归；当前交付范围可开始人工验收，实现及限制见[完成度记录](README.md#completion)。

#### 固定基线

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

#### 核心能力对照

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

##### 业务差异

1. [useGeneralConfig.ts](https://github.com/MetaCubeX/metacubexd/blob/8bbc8f58fef71148a94fb5c0ff808f79b057337d/packages/ui/composables/useGeneralConfig.ts) 的模式修改成功回调由配置页用于关闭连接。TUI 提供显式连接处理选项，默认保留连接；该副作用不进入通用请求层。
2. `useUpdateConfigMutation` 先 PATCH 核心；有 agent `config-sections` 能力时再调用控制接口持久化（restart=false）。持久化失败可能留下已成功的运行修改。纯面板没有该持久化步骤。
3. 上游查询缓存按端点隔离，修改成功后使相关查询失效。TUI 请求与回调已按会话代号隔离。
4. 规则停用已使用核心数字索引实现，缺少 disabled 字段时不执行。

#### Agent 协议边界

依据：[useControlApi.ts](https://github.com/MetaCubeX/metacubexd/blob/8bbc8f58fef71148a94fb5c0ff808f79b057337d/packages/ui/composables/useControlApi.ts)、[agent/http.ts](https://github.com/MetaCubeX/metacubexd/blob/8bbc8f58fef71148a94fb5c0ff808f79b057337d/packages/agent/src/http.ts)、[agent/index.ts](https://github.com/MetaCubeX/metacubexd/blob/8bbc8f58fef71148a94fb5c0ff808f79b057337d/packages/agent/src/index.ts)。

- 控制路径默认是面板同源 `/api/control`；桌面 preload 可提供另一 base/token。仅通过 Mihomo `/ui` 提供静态资源不会自动提供管理路由。
- 使用独立 Bearer token；health/info 是公开探测接口，features 决定能力。事件流也接受查询 token，不能写入普通日志。
- 内核支持 status/start/stop/restart/rollback/recover，另有内核版本、TUN、系统代理、GEO 和日志接口。
- Profile 支持 CRUD、复制、导入、刷新、验证、激活、刷新并激活和编辑快照/预览/保存。类型是 local/remote/merge/script，与 ClashTui File/URL/Template 需要明确转换。
- 编辑器有 revision 与冲突机制；共享管理层应复用或明确适配。
- 还有 config runtime/section、WebDAV backup/restore，应纳入最终能力矩阵。
- Supervisor 面向 Mihomo；与 ClashTui 模板工作流仍需明确适配。

#### 最终实现与核心差异

通用请求使用不可变端点与认证快照；读请求的身份检查缓存两秒，写操作重新检查。端点或认证变化使旧任务失效，旧回调不能污染新会话。非 2xx 统一报错，正文有界且去除当前 secret。活跃页面有间隔和单任务限制，404/405 不被缓存成永久不支持。

连接过滤作用于权威可见行，关闭前固定 ID 集合并报告部分失败。未知元数据保留在详情和导出。日志和采样缓冲有界，WebSocket 使用认证头和系统 TLS 根证书。配置文件校验后原子替换；激活失败恢复旧文件并尝试恢复核心，失败时明确报告。



管理 Web 页面共享本地 Profile、模板、覆盖配置和平台服务函数。数据库写入使用跨进程文件锁，编辑使用内容修订；后台任务恢复已完成/失败结果，中断结果提示核实，不自动重放。没有实现上游 agent 的 script/merge 或 WebDAV 协议；具体交付边界见[功能矩阵](README.md#roadmap)。

CLI 更新资产选择核对了 [Mihomo 官方 v1.19.32 资产列表](https://github.com/MetaCubeX/mihomo/releases/expanded_assets/v1.19.32)：同平台存在 CPU/Go 变体和系统安装包。更新入口选择保守二进制变体并要求 SHA-256；没有把检查发布元数据当成实际升级测试。

#### 验证边界

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

## completion

**目标完成度与开发闭环**


> 2026-10-02 五项实机缺陷已修复：完整隔离流水线通过，真实 VM 32 组通过、0 失败、1 项核心能力跳过。当前验收范围以[修复测试结果](../testing/reports/vm_acceptance_20261002_zh.md)为准；以下保留此前开发记录。

> 最新修复：再次审查发现的三个配置诊断与测速边界问题已修复，完整隔离流水线 9 阶段通过。详见[再次审查记录](README.md#followup)。真实核心与跨平台验收仍待执行。

> 2026-10-01 复审及修复：服务语义、就绪验证、核心 API 能力、模板和诊断结果的七项缺口已修复，并增加 TUI 操作与锁定面板浏览器回归。具体闭环见[三端复审记录](README.md#review)。真实核心与跨平台验收仍待执行。

更新：2026-10-01。**当前交付范围的开发已完成，可以开始人工安装与三端联调测试。** 此前审查列出的事务、缓存、生命周期、激活证据和恢复缺口已处理，并增加对应模拟回归。开发完成不表示真实核心和所有平台已通过验收。

交付范围是固定 **MetaCubeXD v1.273.1** 的纯核心面板，加 ClashTui 本地管理页，与 TUI、CLI 共用本地业务能力。完整入口对应见[三端矩阵](../reference/cli_tui_web_parity_zh.md)。上游 agent/桌面壳的 script/merge、WebDAV 和 `/api/control` 不属于当前项目使用的纯面板部署方式。没有加入任意数量端点档案，本版仅支持 Mihomo 端点配置。

#### 审查问题的关闭结果

| 原问题 | 实现与验证 |
|---|---|
| 文件操作没有完整跨端事务 | `coordination::transaction` 在同一工作线程持有 OS 写锁，贯穿异步下载、生成和激活；已持锁的管理运行时复用事务，避免线程互相等待。开始前刷新并核对 Profile 身份、类型和选项。进程模拟验证旧 CLI 下载期间管理重命名等待，结束后没有旧文件复活 |
| 缓存非原子写入、忽略错误 | Provider/规则缓存统一原子替换并返回保存错误；下载失败保留旧缓存并报告部分失败。标签按固定顺序去重，避免模板标签与生成后缀冲突 |
| 服务命令成功就认为核心已可用 | 启动/重启后检查 Mihomo 身份和配置读取，模拟覆盖错误身份与回读；服务桩验证启动、停止、重启仅操作 Mihomo |
| 激活只证明 API 能访问 | 核心检查通过后才替换；重载后比较可观察设置、节点/组成员、Mihomo 规则数量/顺序/类型及简单规则内容和目标。不匹配触发恢复；恢复回读也校验。模拟覆盖成功响应却仍返回旧节点、模式或规则 |
| 控制地址/密钥迁移仍使用旧会话 | 在替换文件前拒绝改变地址或密钥的在线激活，返回停服迁移说明；重启所有客户端后使用新会话。HTTPS 地址不再被重复加 HTTP 前缀。步骤见[测试指南](../testing/tui_web_testing_zh.md) |
| 配置校验使用固定临时文件 | 独占创建唯一暂存文件，写入错误传播，所有退出路径通过 RAII 清理；不覆盖同名旧文件。数据库编辑也使用同步写入的唯一暂存文件 |
| CLI 更新可能写入错架构或压缩包 | 选择本机平台/架构及保守 CPU 变体，拒绝歧义；要求资产 SHA-256，限制下载/展开大小，解包 GZIP/TAR/ZIP，验证 ELF/Mach-O/PE 架构后暂存替换。Mihomo 使用配置的 `bin_path`。失败保留执行文件；无摘要的旧发布拒绝自动更新 |
| 自定义编辑器解释文件名 | Unix 使用位置参数，处理引号上下文；模板内命令替换明确拒绝，可使用包装脚本。Windows 接受程序路径及可选 `%s`，通过 argv 打开。特殊字符回归通过 |
| Web 状态读取阻塞后续请求 | 非阻塞锁，忙时 HTTP 503；任务轮询仍响应。CLI 管理操作在同一事务内取状态和执行。进程模拟验证锁竞争与响应 |
| 面板版本与任务恢复证据不足 | 展示目标版本和安装记录，未知部署显示未知；记录不等于浏览器实际资源校验。私有文件保存任务结果；重启恢复已完成/失败结果，中断任务提示核实，不自动重放 |

补充修复：删除 Profile 的重复拿数据库锁死锁；模板注册前调用核心检查，提交失败恢复输出，重生成保留 Profile 选项；数据库不可读拒绝提交；首次激活无旧配置且提交失败时明确报告无法恢复。

#### 验证证据

- Rust：351 项单元/模拟接口测试、6 项隔离进程流程通过，0 失败；1 项既有手工交互测试忽略。
- Web：5 项模拟 DOM/fetch、5 项 Chromium 管理页、1 项锁定 MetaCubeXD 页面交互测试通过，均连接临时服务或模拟核心。
- CLI：5 项隔离核心 API 进程测试通过，覆盖认证、HTTP/WebSocket、批次失败、错误核心身份、就绪验证、校验诊断、慢测速响应和完整 URL。
- TUI：3 项真实伪终端测试通过，包含九标签导航、服务启动、自动选组恢复、Provider 单节点测速、模板命名、取消覆盖及非法 YAML 的 Test/Check 诊断。
- 全目标全特性严格 Clippy、格式化及差异空白检查通过。
- 无默认特性 CLI 编译及 144 项单元测试、6 项隔离进程测试通过，仍有未使用代码警告。
- 进程测试只使用临时目录、模拟 HTTP 核心/订阅、服务状态及配置校验桩程序，没有安装或启动真实核心。

测试数量不是完备性证明；关键证据是问题对应实际实现和失败路径回归。执行入口、CI 和覆盖矩阵见[隔离测试流水线](../testing/test_pipeline_zh.md)。首次真实测试从[配对测试指南](../testing/tui_web_testing_zh.md)开始。

#### 人工验收与接口限制

尚待人工验证：真实核心版本差异、锁定 MetaCubeXD 面板资源及真实核心双向同步、真实订阅与网络波动、Linux/macOS/Windows 服务及 Windows 系统代理。三平台 CI 已配置，远端执行结果尚未取得。

Clash API 不返回全部有效配置：节点认证/传输细节、Provider 完整内容不能逐字段证明，仍依赖核心检查和人工连通性验收。复杂逻辑规则只核对顺序/类型，不宣称验证完整表达式。有界启动检查也不能保证核心稍后不会崩溃。

TUI 外部编辑器不参与锁与修订协议，应关闭后再更新/激活；Web 和 CLI 内置保存检查 SHA-256 内容修订。模板及覆盖配置统一使用 Mihomo YAML。

当前阶段：**开发交付完成，等待人工验收；没有声明实机测试已通过。**

单核心迁移：旧数据库中额外的核心数据或旧核心选择会先写入 `clashtui.db.before-mihomo-only` 备份，再按 Mihomo 数据加载。已有 Mihomo Profile 保留，外部核心文件和服务不由此次源码修改删除。

## review

**Mihomo 三端功能与对齐复审**


> 后续审查发现的三个边界问题已修复，并增加非法 YAML、慢响应和完整测速 URL 的回归；最新结果见[再次审查记录](README.md#followup)。下文保留此前七项修复的记录。

审查日期：2026-10-01。初次审查结论：主要业务已有三端入口，但存在下文七项缺口。

#### 本轮修复

七项代码/文档问题已处理：TUI 独立启动和重启；共享服务操作验证 Mihomo 身份及配置就绪；CLI `core unfix GROUP` 和 TUI Proxies `u` 恢复自动选择；CLI `core delay NODE --provider PROVIDER`、TUI Providers `d` 及带 Provider 信息的节点测速使用精确端点；TUI 模板生成可输入名称并确认覆盖；三端配置 Test/Check 保留退出状态及 stdout/stderr；新模板示例改为先读取 revision。

测试增加服务未就绪和诊断失败路径、TUI 实际修改操作、Web 模板生成和覆盖取消，以及经 SHA-256 校验的 MetaCubeXD v1.273.1 页面恢复自动选组操作与 CLI 回读。测试只使用临时数据和模拟核心；真实核心及平台服务验收仍待隔离 VM。

最终完整流水线报告：`target/test-results/1790863588469-7d11fe54/report.json`，9 个阶段通过、0 失败；全特性 Rust 350 单元测试及 6 进程测试、Web DOM 5 项、CLI 4 项、TUI PTY 2 项、浏览器 5 项均通过。CLI 特性另外验证 143 单元测试及相同 6 进程测试，覆盖有重叠；既有 1 项手工测试忽略，实机阶段未执行。TUI 模板生成也已接入共享 management 事务和数据库修订检查。

下文保留初次审查的证据，描述的是修复前行为。

Web 范围为固定 MetaCubeXD v1.273.1 纯面板加 ClashTui 本地管理页。审查依据是当前 Rust/HTML 实现、已有测试源码，以及本机缓存的 MetaCubeXD 源码（package.json 标记 1.273.1）；没有重新查询最新上游。没有实际安装、启动代理核心或修改宿主网络。

#### 需要补齐的项目

##### P1：TUI 的启动入口实际执行重启

`src/tui/tab/srvctl.rs:67` 将 Restart 显示为 “Start Service”，在动作处理中调用 `restart_service()`。没有独立的 Start 动作。CLI 和管理页分别提供 start/restart。

影响：对已经运行的服务点击“启动”会重启并可能中断连接；TUI 也没有语义明确的服务重启入口。应拆分启动和重启，分别调用相同的共享函数，并用服务桩核对实际命令。

##### P1：启动/重启成功不保证核心就绪

`src/functions/command.rs:217`、`:231` 直接返回平台服务命令结果；只有 Profile 激活路径调用 API 就绪等待。TUI 成功回调还直接把状态设置为 active。

隔离复现：将临时 systemctl 桩设为成功，模拟 `/version` 和 `/configs` 返回 503，运行 `service start` 仍成功退出，且没有发送核心 API 请求。

影响：此前完成度文档中“启动/重启后检查身份和配置读取”的声明与当前代码不符。应为三端共用的启动/重启增加有界就绪验证，并区分“服务命令已接受”和“核心可用”。

##### P2：缺少恢复自动选组能力

缓存上游 `packages/ui/composables/useApi.ts:376` 的 `unfixProxyInGroupAPI` 使用 `DELETE /proxies/{group}`；代理页面对存在 fixed 的组提供“恢复自动选择”。当前 Rust 代理接口和 CLI/TUI 只有选择与测速，没有对应解除固定操作。

影响：在 Web 固定自动组节点后，无法通过 TUI/CLI 执行同样的恢复操作。应补共享 API、能力判断、CLI 命令和 TUI 动作。

##### P2：缺少 Provider 范围内单节点测速

缓存上游 `useApi.ts:386` 根据 Provider 使用 `/providers/proxies/{provider}/{node}/healthcheck`。Rust 只有全局 `/proxies/{node}/delay`、组测速及整个 Provider 健康检查，CLI 也没有 Provider 参数。

影响：Provider 中未进入全局代理映射的节点，或不同 Provider 的同名节点，无法由 TUI/CLI 精确测速。整组健康检查不能替代指定节点的结果。

##### P2：模板生成的命名与覆盖策略不同

`src/tui/tab/files/template.rs:390` 固定使用 `{模板文件名}.tpl` 作为 Profile 名称，直接生成；同模板重生成允许覆盖，没有额外确认。CLI/Web 可指定生成名称，覆盖已有生成配置分别需要 `--yes` 和页面确认。

影响：TUI 无法完成与 CLI/Web 相同的“同模板生成多个不同名称配置”工作流，且覆盖保护不同。应统一命名输入、冲突判断和覆盖确认。模板核心校验及写入回滚已经存在，不属于此处缺口。

##### P2：Test 与 Check 的结果语义不同

TUI Test 使用 `test_config()` 展示核心 stdout/stderr；Check 使用 `check_config()` 返回有效性。管理层将 `test` 和 `check` 合并，只返回有效性，成功时不保留核心测试输出。因此 CLI/Web 的“测试”无法查看 TUI 同等诊断信息。

应统一返回退出状态、stdout/stderr 和校验结果，再由三端选择展示形式。

##### P2：新模板命令示例失效

`docs/reference/cli_tui_web_parity_zh.md` 使用 MD5 空内容值 `d41d8cd98f00b204e9800998ecf8427e`；实现 `management::revision()` 使用 SHA-256。按示例在隔离目录执行，返回 `Revision conflict: reload the document before saving`。

建议示例先执行 `manage read --kind template --name custom.yaml`，再使用其返回的 revision，避免再次硬编码旧算法。

#### 已具备的主要能力

节点选择与普通测速、规则及 Provider 查询/更新、连接查询/关闭、日志、指标、运行设置、Profile 导入/更新/激活、覆盖配置、模板、服务和固定面板部署均能找到相应入口。CLI/Web 大量复用 management；TUI 部分复用 management，部分直接调用更底层函数。因此“共用 Rust 代码”不能自动证明每个操作的确认、返回结果和失败处理一致。

TUI 外部编辑器与 CLI/Web 修订保存的并发保障差异、CLI 有限采样与界面持续展示的差异，属于需要明确记录的交互边界。Windows 系统代理和服务注册按平台提供。上游桌面壳及 agent 的 WebDAV/script/merge 仍属于原先排除的范围。

#### 测试证据和缺口

- 上次完整流水线：9 阶段通过，350 项全特性单元测试、6 项隔离进程测试；CLI 特性测试与其有重叠。此次未重复运行整套流水线。
- 此次额外执行上述两项定向隔离复现：旧模板 revision 被拒绝、API 不可用时服务启动仍成功。
- 现有 Chromium 三个场景只覆盖本地管理页的认证、导入/保存/导出、冲突、取消和运行设置；没有加载 MetaCubeXD 页面。
- 现有 TUI PTY 只验证启动、九标签导航、帮助和退出，不执行修改操作。
- 缺少逐项“三端操作 → 另两端读取”的一致性回归，特别是模板生成、服务生命周期、节点解除固定及 Provider 单节点测速。
- 实际 Mihomo、锁定面板资源、Windows/macOS 服务和代理网络仍需隔离 VM 验收。

建议顺序：先修服务语义与就绪检查，再补两个核心 API 能力，统一模板和测试结果，修正文档示例；最后增加三端共享场景与锁定面板浏览器回归，再进行隔离 VM 验收。

## followup

**七项修复后的再次审查**


> 本文三项修复已验证；后续发现的四项 Provider 问题尚未关闭，见[Provider 工作流再次审查](README.md#providers)。

#### 修复闭环（2026-10-01）

下述三个 P2 问题已修复，并通过完整隔离流水线。后文保留修复前的审查证据。

- TUI Test/Check 改为在后台调用共享管理动作，沿用事务、修订和文件检查，不再提前解析 YAML。非法 YAML 回归验证 CLI、TUI、Web 均调用校验器并返回核心诊断；TUI 两次操作通过校验器调用记录确认。
- 普通节点、组和 Provider 单节点测速使用独立 HTTP 超时：`max(20000, timeout + 10000)` 毫秒，向上取整为秒。测速预算接受 1～3,600,000 毫秒；核心身份检查仍使用全局超时。全局 2 秒、测速预算 5 秒、响应延迟 2.6 秒的三种测速均成功；无效预算回归通过。
- 查询参数按非保留字符编码，包括转义 `%`。回归验证服务端解析后的完整测速 URL 与输入一致，覆盖已有百分号编码、中文、多个查询参数及片段。

完整报告：`target/test-results/1790898492247-14a5b531/report.json`。9 阶段通过、0 失败；全特性 Rust 351 项单元测试及 6 项进程测试、CLI 特性 144 项单元测试及相同 6 项进程测试、Web DOM 5 项、CLI 核心流程 5 项、TUI PTY 3 项、浏览器 6 项通过。两种 Rust 特性覆盖有重叠，既有手工交互测试各忽略 1 项；真实核心与平台阶段未执行。

修复与测试仅使用临时目录、回环模拟 API 和服务/校验桩，没有安装或启动真实代理，也没有修改系统服务或网络。真实核心及跨平台服务仍待隔离 VM 验收。

#### 修复前的审查记录

日期：2026-10-01。结论：此前七项问题的主要入口和正常路径已实现，但配置诊断、Provider 测速仍有边界遗漏。不能据上次 9 阶段通过认定三端行为完全一致。

本次仅审查并执行临时目录、服务桩和回环模拟 API 的定向复现，没有修改业务代码、启动真实代理或修改系统服务/网络。

#### P2：非法 YAML 时，TUI 不返回核心校验诊断

位置：`src/tui/tab/files/profile.rs:996`、`:1012`，`src/functions/file/profile/profile.rs:6`。

TUI Test/Check 在调用共享 `configuration_test()` 前先执行 `load_local_profile()`，该函数将文件解析为 YAML Mapping。文件语法错误时提前返回 Rust YAML 解析错误，不会调用核心。CLI/Web 的管理动作直接将文件路径传给核心，因此会返回核心的退出码和 stdout/stderr。

复现：先导入合法 Profile，再模拟外部编辑器把文件改成 `[unterminated`。临时校验程序每次调用记录标记并输出 `CORE-DIAGNOSTIC`。CLI Test 返回 `valid=false`、`exit_code=1` 和该 stderr；随后用实际 TUI 伪终端进入 Files 执行 Test，TUI 正常退出，但校验器调用次数仍为 1。源码也确认 Check 使用相同的提前解析路径。

建议：TUI 通过共享管理动作获取文件并测试，避免诊断前先解析内容；增加非法 YAML 的 TUI/CLI/Web 对照测试。现有诊断回归使用合法 YAML 配合失败的校验桩，无法覆盖此路径。

#### P2：测速请求的 HTTP 超时短于核心测速预算

位置：`src/functions/restful/proxies.rs:167`、`src/functions/restful/session.rs:181`。

Provider 单节点测速把 timeout 放入查询参数，但 HTTP 请求仍使用全局 `CONFIG.cfg_file.timeout`。提高 CLI `--timeout` 不会延长客户端等待；TUI 的测速预算与全局 HTTP 超时相同，也没有往返开销余量。普通节点/组测速同样经过这一请求层。

复现：全局超时 2 秒，运行 `core delay node --provider provider --timeout 5000`，模拟核心在 2600 毫秒后返回有效延迟。CLI 在约 2013 毫秒失败，输出 `the timeout of the request was reached`。缓存的锁定 MetaCubeXD 源码使用 `max(20000, timeout + 10000)` 毫秒的客户端等待，行为不同。

建议：请求层支持单次超时，按测速预算加往返余量设置，并验证单位转换和上限；增加延迟响应回归。现有测速 mock 立即返回，无法发现提前超时。

#### P2：测速 URL 中已有的百分号编码被改变

位置：`src/functions/restful/proxies.rs:19`，Provider、普通节点和组测速均调用 `encode_query()`。

该函数将 `%` 原样保留。作为外层查询参数发送后，服务端解析会把原测速 URL 中的 `%2F`、`%26` 等解码，改变真正的测速目标。路径名的编码函数不受此问题影响。

复现：请求测速 URL 为 `https://example.test/a%2Fb?key=x%26y`，模拟核心解析收到的 url 参数为 `https://example.test/a/b?key=x&y`。带编码路径或签名查询参数的测速目标可能因此失效。

建议：使用标准查询参数构造器，确保包括 `%` 在内的参数值正确转义；断言服务端解码后的完整 URL 与用户输入完全一致。

#### 本轮未发现同级问题的修复

- TUI 独立启动/重启及 CLI/Web 的共享服务函数已接通；服务就绪路径检查版本身份和运行配置。
- 解除自动组固定选择已接入 CLI/TUI；锁定 MetaCubeXD 页面已有操作及 CLI 回读回归。
- TUI 模板生成支持独立名称、覆盖确认，并使用共享管理事务及数据库修订检查。
- 新模板示例已改为读取实际 revision。

这些是本次审查范围内的结论，不等于所有并发状态或平台均已穷举。现有完整流水线结果仍有效，但未覆盖本次三个边界；本次没有重复执行整套流水线。真实核心及跨平台服务仍待隔离 VM 验收。

## providers

**Provider 工作流再次审查**


当前状态：2026-10-02 四项 Provider 缺陷均已修复，并与只读 CLI 修订冲突一起通过实机回归。最终行为和证据见[修复验收](../testing/reports/vm_acceptance_20261002_zh.md)。以下保留修复前发现与复现记录。

历史日期：2026-10-01。本轮为代码审查与隔离复现，没有修改业务代码，没有安装或启动真实代理，没有修改系统网络。上一轮配置诊断、测速等待和 URL 编码三项修复仍有效；但新发现以下四项问题，尚未修复，不能据现有流水线宣称所有边界已闭环。

#### P1：Provider 下载路径未限制在核心配置目录内

位置：`src/functions/file/template.rs:749`、`:777`，`src/functions/file/net_resource.rs:106`。

资源提取直接接受配置的 `path`；预下载通过 `config_dir.join(path)` 构造路径后原子写入。没有拒绝 `..`、绝对路径或检查祖先目录的符号链接。普通 Profile 更新和激活预下载均经过此函数；内联 Provider 另有直接拼接路径的缓存写入，也应一并检查。

隔离复现：在临时核心目录 `mihomo/` 的父目录创建 `sentinel.yaml`，内容为 `keep-existing-file`。导入包含 `proxies: []` 和 HTTP rule-provider 的配置，设置 `path: ../sentinel.yaml`；回环订阅返回合法 YAML。执行 `manage update` 返回 `partial_failure=false`，父目录文件被替换为 `payload: [example.test]`。没有接触宿主真实文件。

这使订阅配置可以借 Rust 预下载覆盖当前进程有权限写入的目录外文件。Mihomo 本身默认将 Provider 路径限定于 HomeDir，额外位置通过 SAFE_PATHS 指定，Rust 预下载绕过了这一限制。参见[官方规则集合文档](https://wiki.metacubex.one/config/rule-providers/)。

建议：所有 Provider 下载与缓存写入共用路径校验器，默认限定于核心配置目录，检查规范化后的目标和祖先符号链接；如果支持额外安全目录，应采用明确的授权目录配置。加入相对越界、绝对路径、祖先符号链接和正常子目录回归。

#### P2：合法的 text/mrs 规则集被按 YAML Mapping 校验

位置：`src/functions/file/template.rs:763`、`src/functions/file/net_resource.rs:21`。

资源模型没有携带 rule-provider 的 `format`/`behavior`；预下载对代理和规则资源一律解析为 YAML Mapping。Mihomo 支持 yaml、text、mrs 格式，见[官方规则集合文档](https://wiki.metacubex.one/config/rule-providers/)。

隔离复现：导入 `format: text`、`behavior: domain` 的 HTTP rule-provider，回环服务器返回 `example.test` 和 `+.example.org` 两行。`manage update` 返回 `partial_failure=true`、`Invalid YAML format`，退出码 1，不写入合法规则文件。mrs 也会被同一校验逻辑拒绝，本轮只实际复现 text。

建议：提取并保留资源格式，按代理/规则、yaml/text/mrs 分别校验。内联规则功能需针对格式解析或明确报不支持，不能将非 YAML 格式统一视为下载损坏。

#### P2：模板 Provider 下载失败且有缓存时报告更新成功

位置：`src/functions/file/profile.rs:406`。

模板 Profile 更新下载错误时，只要旧缓存可解析为 Mapping，就返回 `ok=true,error=null`，丢失本次网络错误。共享管理层因此返回 `partial_failure=false`，CLI 退出 0，Web/TUI 也无法展示这次失败。

隔离复现：生成含 Provider 分组的模板 Profile，预置合法缓存；回环订阅返回 HTTP 401。执行 `manage update` 输出资源 `ok=true,error=null`、`partial_failure=false`，退出码 0。旧缓存被保留，但没有成功更新。

建议：区分“本次更新成功”和“旧缓存可用”，保留失败原因并明确缓存回退状态。单次与批量更新的三端结果都应显示部分失败，而不是把缓存可用计为下载成功。

#### P2：显式清空 Provider 分组后仍回退到旧分组

位置：`src/functions/file/template.rs:174`。

`read_template_ppg()` 只有在分组非空时接受模板内值；显式空对象与缺少该字段都回退到旧 `template_proxy_providers.yaml`。因此用户通过 Web/CLI 保存 `{}` 或通过 TUI 编辑清空后，旧分组再次生效，后续生成仍使用这些订阅。

隔离复现：模板包含 `clashtui.proxy_provider_groups: {}`，旧文件包含 `legacy.old` 指向回环订阅。`manage template_providers` 返回旧 `legacy` 分组，而不是 `{}`。

建议：仅字段缺失时执行兼容回退；`Some(empty)` 应表示明确清空。加入旧文件存在时保存、读回、预览和生成的回归。

#### 验证范围

以上四项均以当前 `target/debug/clashtui` 二进制、临时配置目录、假校验程序及回环模拟订阅复现。真实核心未执行；mrs、符号链接路径属于源码确认的同路径风险，未另行运行实机验证。本轮未重复完整流水线；此前 9 阶段通过的记录仍有效，但缺少这些输入和失败路径。

## profile-editor

**TUI 订阅编辑问题与项目背景**


本文记录 Files 页面订阅编辑问题的原因、当前实现和这次 TUI 改进的范围，供后续开发与排查使用。用户操作说明见[使用指南](../guides/getting_started_zh.md#tui-界面)，代码整体结构见[架构文档](../development/architecture_zh.md)。

#### 项目背景

ClashTui 是 Rust 终端界面程序，管理 Mihomo 代理核心。它通过 CLI 子命令支持脚本操作，也提供 TUI 用于交互式管理。

启动入口在 `src/main.rs`：解析参数后初始化配置；有 CLI 子命令时直接处理，否则初始化主题、按键与终端，再启动 TUI。配置目录由 `--config-dir` 或 `CLASHTUI_CONFIG_DIR` 指定，也可以使用默认的用户配置路径。

配置数据分成几类：

| 数据 | 文件 | 用途 |
|---|---|---|
| 程序与服务配置 | `config.yaml` | 核心可执行文件、配置路径、服务和外部编辑命令 |
| Profile 数据库 | `clashtui.db` | 当前核心、Profile 类型、订阅 URL、选中 Profile 和更新选项；文件内容是 YAML |
| Mihomo 覆盖配置 | `mihomo/core_override_config.yaml` | 选择 Mihomo Profile 时覆写顶层配置项 |
| Profile 文件 | 各核心目录下的 `profiles/` | 下载的订阅配置或本地导入配置 |

`ProfileManager` 保存 Mihomo Profile 列表和当前选择；名称、文件和选中项在同一事务内维护。

#### TUI 与按键流程

`App::serve()` 每秒约渲染 50 帧，并接收按键、窗口尺寸变化和异步任务完成事件。按键依次经过弹窗、全局组合键、帮助面板、标签页组合键、当前标签页和全局按键。

Files 是 `DualTab<Profile, Template>`：左侧管理订阅，右侧管理模板。两边共享当前焦点，窄终端应优先显示有焦点的面板。Profile 和 Template 分别定义默认按键；`keymap.yaml` 可以按核心、标签页覆盖按键，也支持多键组合。

输入框由弹窗接管按键，直到用户按 Enter 或 Esc。配置文件外部编辑则由 `extra.edit_cmd` 处理，通常用于启动 Vim 或系统编辑器。

#### `e` 偶尔不可用的原因

原来的 `e` 绑定其实存在，但执行的是“打开下载后的 YAML / JSON 文件”，不是用户期望的“编辑订阅名称或 URL”。当文件不存在、编辑命令配置错误或编辑器无法启动时，用户会看到 `e` 没有达到预期；一部分命令以分离进程启动，错误反馈也不充分。

按键路由中还有几个会让功能间歇失效的条件：

1. **组合键前缀吞掉普通按键。** 如先按 `g` 进入 `gg` 组合键，再按 `e`，原处理器会把不匹配的 `e` 消耗掉，不再交给单键快捷键处理。自定义三键组合也可能在只匹配到中间前缀时提前执行。
2. **局部按键配置替换了整份默认绑定。** 旧逻辑只要加载到一个自定义绑定，就只查用户配置的映射。例如只改 `j`，默认的 `e`、`i` 等功能也会消失。
3. **筛选行号被当成数据库索引。** 界面先筛选 `items`，但动作仍以可见列表中的行号访问未筛选列表。它可能编辑、更新或删除另一条 Profile；空筛选结果也容易产生无效选中。
4. **Profile 身份查找缺少明确约束。** 当前实现只管理 Mihomo，查找、修改和删除均使用同一 Profile 集合，并检查名称及修订。
5. **筛选和输入交互缺少反馈。** 搜索后光标可能超出可见范围；FZF 返回的是筛选列表索引；预填文本缺少可靠的清空和光标操作；模板预览尚未实现。
6. **随安装提供的按键文件与当前动作枚举有旧名字差异。** 原配置里的 `TrafficNext` / `TrafficPrev` 不再是当前动作名，可能使按键配置加载失败。

这些因素会让“看起来按了按键，却没有执行对应功能”呈现为偶发问题；还可能让某条编辑等操作实际命中错误订阅。

#### 当前修改

在 Profile 列表按 `e` 打开名称编辑框；若 Profile 类型是 URL，再打开 URL 编辑框。两个字段都预填当前值。输入无效时提示原因并保留可修正的值；任一环节按 Esc 均取消整个编辑。名称验证会阻止空名称、路径分隔符和平台保留字符，URL 仅接受带主机的 HTTP 或 HTTPS 地址。

确认保存时先构造数据库副本，再写临时数据库文件；改名时同步重命名本地 Profile 文件。持久化失败会尝试恢复原文件名，不覆盖已有目标。当前 Profile 名称变化时同步当前选择，更新选项也随 Profile 保留。改 URL 不会自动联网下载；保存后按 `u` 拉取新地址。

按 `E` 仍表示打开配置文件。File、URL Profile 可编辑本地 Mihomo YAML。名称和 URL 的修改在 TUI 内完成，不依赖 `extra.edit_cmd`。

同时已修改按键分发、列表筛选和反馈：

- 默认按键与用户的局部覆盖合并；显式自定义组合键可以覆盖同前缀默认单键。
- 组合键前缀不匹配时恢复到普通按键处理；完整多键组合匹配后才触发动作。
- Profile 与 Template 的动作、FZF 和光标都按可见列表映射；空结果取消选中，Enter 可生成模板。
- Files 面板显示焦点、数量、选中位置和空列表说明；页脚按屏幕宽度展示按键，窄屏只展开当前标签名。
- 加入筛选提示、Esc 清除筛选、编辑成功反馈、直接按键预览模板、预填输入框及 Home / End / Ctrl-U 操作。
- 同名 Profile 的查找、修改、删除按活动核心隔离；导入后选中新 Profile。模板预览不再触发未实现分支。
- 旧 Traffic 快捷键名称可以读取，并映射到当前可用的流量显示动作。

#### 改动入口

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

#### 验证记录与限制

- `cargo test --locked --all-features --no-fail-fast`：**350 单元测试及 6 进程测试通过、0 失败、1 忽略**。
- `cargo check --locked --all-features`：通过。
- `cargo clippy --locked --all-targets --all-features -- -D warnings`：通过。为修复现有 macOS Clippy 告警，调整了几个条件表达式，并移除了原来恒真的平台测试。
- `cargo fmt --all -- --check`、`git diff --check`：通过。
- 当前 Mihomo 隔离流水线通过 TUI 伪终端九标签冒烟及订阅/模板 Rust 回归；历史手工交互记录不作为当前版本实机证据。

命令行测试运行时曾触发 macOS 测试临时目录同纳秒碰撞；生成目录现改用随机值。config 路径的 macOS 单测也改为从模块作用域引用私有函数。

本机伪终端验证采用短生命周期临时工作目录，模拟数据不会影响真实订阅或服务。编辑器外部进程的行为未包含在该伪终端流程中；`E` 仍按本机 `extra.edit_cmd` 或系统默认程序打开配置文件。
