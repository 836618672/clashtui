# 七项修复后的再次审查

> **历史归档**：本文保留当时的规划、发现和验证范围；正文中的“尚未修复/测试”等结论属于历史状态。当前状态见[项目状态](../reference/project_status_zh.md)，后续修复与实机证据见[2026-10-02 验收报告](../testing/reports/vm_acceptance_20261002_zh.md)。

> 本文三项修复已验证；后续发现的四项 Provider 问题尚未关闭，见[Provider 工作流再次审查](three_client_provider_review_zh.md)。

## 修复闭环（2026-10-01）

下述三个 P2 问题已修复，并通过完整隔离流水线。后文保留修复前的审查证据。

- TUI Test/Check 改为在后台调用共享管理动作，沿用事务、修订和文件检查，不再提前解析 YAML。非法 YAML 回归验证 CLI、TUI、Web 均调用校验器并返回核心诊断；TUI 两次操作通过校验器调用记录确认。
- 普通节点、组和 Provider 单节点测速使用独立 HTTP 超时：`max(20000, timeout + 10000)` 毫秒，向上取整为秒。测速预算接受 1～3,600,000 毫秒；核心身份检查仍使用全局超时。全局 2 秒、测速预算 5 秒、响应延迟 2.6 秒的三种测速均成功；无效预算回归通过。
- 查询参数按非保留字符编码，包括转义 `%`。回归验证服务端解析后的完整测速 URL 与输入一致，覆盖已有百分号编码、中文、多个查询参数及片段。

完整报告：`target/test-results/1790898492247-14a5b531/report.json`。9 阶段通过、0 失败；全特性 Rust 351 项单元测试及 6 项进程测试、CLI 特性 144 项单元测试及相同 6 项进程测试、Web DOM 5 项、CLI 核心流程 5 项、TUI PTY 3 项、浏览器 6 项通过。两种 Rust 特性覆盖有重叠，既有手工交互测试各忽略 1 项；真实核心与平台阶段未执行。

修复与测试仅使用临时目录、回环模拟 API 和服务/校验桩，没有安装或启动真实代理，也没有修改系统服务或网络。真实核心及跨平台服务仍待隔离 VM 验收。

## 修复前的审查记录

日期：2026-10-01。结论：此前七项问题的主要入口和正常路径已实现，但配置诊断、Provider 测速仍有边界遗漏。不能据上次 9 阶段通过认定三端行为完全一致。

本次仅审查并执行临时目录、服务桩和回环模拟 API 的定向复现，没有修改业务代码、启动真实代理或修改系统服务/网络。

## P2：非法 YAML 时，TUI 不返回核心校验诊断

位置：`src/tui/tab/files/profile.rs:996`、`:1012`，`src/functions/file/profile/profile.rs:6`。

TUI Test/Check 在调用共享 `configuration_test()` 前先执行 `load_local_profile()`，该函数将文件解析为 YAML Mapping。文件语法错误时提前返回 Rust YAML 解析错误，不会调用核心。CLI/Web 的管理动作直接将文件路径传给核心，因此会返回核心的退出码和 stdout/stderr。

复现：先导入合法 Profile，再模拟外部编辑器把文件改成 `[unterminated`。临时校验程序每次调用记录标记并输出 `CORE-DIAGNOSTIC`。CLI Test 返回 `valid=false`、`exit_code=1` 和该 stderr；随后用实际 TUI 伪终端进入 Files 执行 Test，TUI 正常退出，但校验器调用次数仍为 1。源码也确认 Check 使用相同的提前解析路径。

建议：TUI 通过共享管理动作获取文件并测试，避免诊断前先解析内容；增加非法 YAML 的 TUI/CLI/Web 对照测试。现有诊断回归使用合法 YAML 配合失败的校验桩，无法覆盖此路径。

## P2：测速请求的 HTTP 超时短于核心测速预算

位置：`src/functions/restful/proxies.rs:167`、`src/functions/restful/session.rs:181`。

Provider 单节点测速把 timeout 放入查询参数，但 HTTP 请求仍使用全局 `CONFIG.cfg_file.timeout`。提高 CLI `--timeout` 不会延长客户端等待；TUI 的测速预算与全局 HTTP 超时相同，也没有往返开销余量。普通节点/组测速同样经过这一请求层。

复现：全局超时 2 秒，运行 `core delay node --provider provider --timeout 5000`，模拟核心在 2600 毫秒后返回有效延迟。CLI 在约 2013 毫秒失败，输出 `the timeout of the request was reached`。缓存的锁定 MetaCubeXD 源码使用 `max(20000, timeout + 10000)` 毫秒的客户端等待，行为不同。

建议：请求层支持单次超时，按测速预算加往返余量设置，并验证单位转换和上限；增加延迟响应回归。现有测速 mock 立即返回，无法发现提前超时。

## P2：测速 URL 中已有的百分号编码被改变

位置：`src/functions/restful/proxies.rs:19`，Provider、普通节点和组测速均调用 `encode_query()`。

该函数将 `%` 原样保留。作为外层查询参数发送后，服务端解析会把原测速 URL 中的 `%2F`、`%26` 等解码，改变真正的测速目标。路径名的编码函数不受此问题影响。

复现：请求测速 URL 为 `https://example.test/a%2Fb?key=x%26y`，模拟核心解析收到的 url 参数为 `https://example.test/a/b?key=x&y`。带编码路径或签名查询参数的测速目标可能因此失效。

建议：使用标准查询参数构造器，确保包括 `%` 在内的参数值正确转义；断言服务端解码后的完整 URL 与用户输入完全一致。

## 本轮未发现同级问题的修复

- TUI 独立启动/重启及 CLI/Web 的共享服务函数已接通；服务就绪路径检查版本身份和运行配置。
- 解除自动组固定选择已接入 CLI/TUI；锁定 MetaCubeXD 页面已有操作及 CLI 回读回归。
- TUI 模板生成支持独立名称、覆盖确认，并使用共享管理事务及数据库修订检查。
- 新模板示例已改为读取实际 revision。

这些是本次审查范围内的结论，不等于所有并发状态或平台均已穷举。现有完整流水线结果仍有效，但未覆盖本次三个边界；本次没有重复执行整套流水线。真实核心及跨平台服务仍待隔离 VM 验收。
