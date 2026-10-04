# 隔离测试流水线

整体执行顺序、真实 VM、待补自动断言及最小人工范围见[mini PC 自动闭环方案](README.md)。本文说明 A 层模拟回归；无图形环境可使用无头 Chromium，无需 Mac。

默认流水线不安装或启动真实代理核心，不修改 TUN、DNS、路由、系统代理或本机服务。流程使用临时配置目录、绑定 `127.0.0.1:0` 的模拟核心，以及临时 PATH 中的服务和核心校验桩。服务桩流程只在 Linux 运行，避免其他平台调用真实服务管理器。

## 本地运行

依赖：Rust（项目最低版本 1.89）、rustfmt、clippy、Node.js 22+；Linux 的真实终端冒烟测试还需要 Python 3。已有 Cargo/npm 依赖缓存时，测试应用的通信只使用回环地址；首次获取开发依赖可能联网。

```sh
node scripts/test-pipeline.mjs
```

脚本先执行 npm ci、Vue 类型检查/构建、Vitest 和构建产物一致性校验，再执行格式检查、严格 Clippy、全部特性 Rust 测试、无默认特性 Rust 测试、TUI 构建、CLI 模拟核心流程和 TUI 伪终端测试。失败后继续收集其他阶段结果，最终以非零状态退出。每阶段最多 20 分钟；Unix 中超时或取消会终止该阶段的进程组。

结果保存在 `target/test-results/<运行编号>/`：`report.json`、`report.md` 和每阶段日志。可通过 `CLASHTUI_REPORT_DIR` 指定目录。报告分别列出 passed、failed、skipped；未执行项目不计为通过。`CARGO_TARGET_DIR` 会通过 Cargo metadata 解析，进程用例使用本次构建的二进制。

浏览器层使用固定版本 Playwright 和锁文件，需要单独准备浏览器测试依赖：

```sh
npm ci --prefix tests/browser
cd tests/browser
npx playwright install chromium
cd ../..
node scripts/test-pipeline.mjs --browser
```

也可用 `CLASHTUI_CHROMIUM_PATH` 指定已有 Chromium。单独执行 `npm test --prefix tests/browser` 时，先运行 `cargo build --locked --all-features`；自定义目标目录时显式设置 `CLASHTUI_TEST_BINARY`。浏览器仅访问临时 Vue 面板；只有 Rust 服务连接回环模拟核心，阻断页面向外部地址发出的请求。失败保留 trace、截图和 HTML 报告。

面板已经内置；不再需要 MetaCubeXD 发布归档，不下载上游面板。浏览器用例验证统一认证、节点选择与恢复自动选择、节点/组/Provider 测速、资源与规则能力、连接筛选/详情/导出/固定集合关闭、设置/持久化/DNS、维护成功与失败、实时流重连和日志有界缓存。它们使用模拟核心，真实 VM 结果另列。

## 自动覆盖范围

### 单元测试的单独运行

已有依赖时，可从项目根目录运行：

```sh
npm --prefix web test
npm --prefix web run typecheck
cargo test --all-features
```

前端的 `web/src/store.test.ts` 使用可控 Promise 和虚拟时钟验证任务等待、失败、替换、取消、重复提交、跨会话迟到响应，以及日志/采样缓存上限与重置；`live.test.ts` 驱动 Vue 生命周期钩子，验证暂停、恢复、卸载、错误去重和过期请求；`settings-patch.test.ts` 验证字段白名单、嵌套编辑和零值。它们不依赖真实核心或浏览器。Rust 管理模块测试使用自动清理的临时文件，验证非法编辑保留原文、新文件修订校验、连续保存冲突及运行设置持久化边界。

2026-10-04 本轮新增 27 项单测（前端 22、Rust 5）。前端共 35 项通过；全部特性 Rust 共 365 项单测和 7 项进程测试通过，1 项人工交互测试保持忽略。新增回归先复现、再修复了旧操作失败、旧轮询错误和旧登录请求干扰新会话状态的三处竞态。另有 5 项登录/通知浏览器回归及严格 Clippy 通过；本轮未运行完整 VM 流水线。已有测试流水线会自动发现这些测试，无需新增依赖。

| 功能 | 自动验证 | 边界 |
|---|---|---|
| CLI 参数及三端公共业务分发 | Rust 测试，CLI 子进程用例 | 不等于每种组合均已穷举 |
| 配置导入、订阅、读写、改名、删除、模板/Provider 生成 | Rust 单元与隔离管理流程 | 核心校验是桩；真实订阅另验 |
| 修订冲突、Profile 文件隔离、锁竞争、失败批次 | Rust 进程与浏览器双编辑器 | 外部编辑器不参与修订协议 |
| 配置激活、身份识别、等待生效、失败恢复 | 模拟 API 与假的 systemctl 进程 | 不证明实际核心加载配置成功 |
| 节点选择、节点/组/Provider 单节点测速 | CLI HTTP 用例，慢响应、URL 编码往返及超时边界回归 | 延迟返回模拟数据 |
| 非法 YAML 的 Test/Check 诊断 | CLI、真实 TUI PTY、Chromium 管理页对照及校验器调用记录 | 使用失败校验桩，不证明真实核心诊断内容 |
| 连接原始字段、过滤、关闭目标范围 | CLI 子进程、Rust 逻辑 | 只关闭捕获的模拟连接 ID |
| 规则开关、代理/规则 Provider、健康检查 | CLI HTTP，404/500 失败路径 | 实际核心支持程度需验收 |
| 日志、内存、流量指标 | CLI HTTP/WebSocket 与 Rust 流测试 | 不覆盖长时间压力及真实网络波动 |
| 运行设置修改/持久化、错误密钥、错误核心身份 | CLI/浏览器、Rust 校验 | 修改模拟状态和临时覆盖文件 |
| 缓存清理、Geo 更新、核心重启/升级 | CLI 模拟路由与共享逻辑 | 没有执行真实更新或重启 |
| TUI 启动、九标签切换、帮助、退出 | 真实进程与 Python PTY；Rust 键盘/生命周期测试 | PTY 冒烟不替代每项交互验收 |
| 管理页认证、导入、保存、导出、筛选、取消删除 | Chromium + 实际管理 HTTP 服务 | 默认需显式启用浏览器阶段 |
| 浏览器并发编辑、非法内容、临时设置/持久化、取消编辑 | Chromium + CLI 文件读回 | 不改变运行服务 |
| 面板版本、归档安全、完整性与部署事务 | Rust 模拟测试 | 不在默认流程下载/部署面板 |
| Linux/macOS/Windows 编译与通用逻辑 | CI 三平台，严格 Clippy、两种特性组合 | 本地 Linux 结果不代表其他平台已通过 |

## CI

`.github/workflows/test_pipeline.yml` 在 PR、main/dev 相关代码变更及手动触发时运行：

- 三平台执行默认流程并上传 JSON、Markdown 和日志。
- Ubuntu 单独执行 Chromium 浏览器层，失败上传 trace 和截图。
- Rust 1.89 检查全部特性及 CLI 构建，检查最低版本兼容性。

浏览器下载发生在 GitHub 的临时 runner。CI 不安装代理服务，不操作系统代理，不启用 TUN。Linux 专属进程测试在 macOS/Windows 报告 skipped；通用 Rust 检查照常执行。仓库上传并触发工作流后才能获得远端平台结果。

## 本次执行结果

2026-10-02 在 Linux 执行含浏览器层及锁定面板归档的完整流程：**9 个阶段通过、0 失败、1 个默认实机阶段跳过**。报告：`target/automated-results/20261002T140652Z/isolated/report.json`（2026-10-02 14:06 UTC 起重跑，本地生成文件，不提交仓库）。独立的真实 VM 与最终恢复结果见[本轮自动回归](reports/automated_acceptance_20261002_zh.md)，不能与 mock 结果合并宣称全部通过。

- 全特性 Rust：354 项单元/模拟测试与 7 项进程测试通过，1 项手工交互测试忽略。
- CLI 特性 Rust：146 项单元测试与 7 项进程测试通过，1 项手工交互测试忽略；这部分与全特性覆盖有重叠。
- Vue 单元：6 项通过；CLI 核心流程：5 项通过，新增慢测速响应与 URL 编码往返；TUI PTY：3 项通过，包含模拟核心/服务操作、模板命名、取消覆盖及非法 YAML 诊断。
- Chromium：5 项管理页及 1 项锁定 MetaCubeXD 页面测试通过，新增非法 YAML 的 Web/CLI 诊断对照；本地使用已有 Nix Chromium，CI 使用 Playwright 对应 Chromium。
- Bash 安装脚本：另行执行 22 项隔离单元测试通过，只测试参数和临时文件，不进行实际安装。

本地 Rust 1.89.0 两种特性的最低版本编译检查此前已通过，本次重跑使用 Rust 1.95.0。远端三平台 CI、macOS/Windows 实机验收尚未执行；本地 Linux VM 五项缺陷已修复，32 组真实核心测试通过、1 项核心能力跳过；具体范围见验收记录。

## 必须在隔离 VM 验收的功能

本机 Linux 虚拟机已建立，启动、访问、快照与恢复见[本地虚拟机环境](local_vm_testing_zh.md)。其真实核心冒烟证据独立保存，不改变默认 mock 流水线的实机阶段 skipped 状态。

真实服务安装/卸载与自启、Windows 系统代理、TUN/路由/DNS、真实核心加载和恢复、内置 Vue 面板、真实订阅网络错误与更新、核心/Geo 升级无法由 mock 证明。默认报告将这一层明确标记为 skipped，不自动执行安装脚本。

建立带快照的 Linux/macOS/Windows 测试机；使用 NAT，禁止桥接和宿主配置目录共享。每项从快照恢复，用独立账号、核心配置及测试令牌。按[自动闭环方案](README.md)分配真实自动断言和专项。M01–M30 只作覆盖/复现参考，不要求全部手工操作。记录核心版本、操作前后服务状态、配置文件、网络接口、路由、DNS、截图和日志。至少包含：

1. 无 TUN、DNS 监听和系统代理的显式代理模式；验证配置、节点、连接、规则、Provider 和日志在三端互相可见。
2. Mihomo 服务启动、停止、重启；成功激活、校验失败、API 身份不符、等待超时和恢复原配置。
3. 单独快照下验证系统服务、Windows 代理、TUN、DNS 与重启自启；检查停止/卸载后网络恢复。
4. 固定版本面板部署与 SHA-256、订阅授权/重定向/超时、升级失败后的恢复。

验收记录应区分 mock、浏览器和 VM 证据；没有 VM 记录时不得将这一层写成“全部功能实机通过”。

## 正式验收入口

本页描述 A 层。正式验收使用 `bash scripts/acceptance.sh`，将 A、安装器、真实 B/C、现场归档、最终恢复和宿主对照串联。完整映射与跳过边界见[覆盖契约](acceptance_coverage_zh.md)，操作见[测试方案](README.md)。

最新完整 A/B/C 结果为[2026-10-02 C 层验收报告](reports/c_acceptance_20261002_zh.md)：真实 VM 64 组通过、0 失败、1 项核心能力跳过；30 组新增 C 用例全部通过，源码和宿主对照及恢复通过。
