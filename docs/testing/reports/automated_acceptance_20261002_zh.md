# 2026-10-02 mini PC 自动回归结果

本轮按[自动闭环方案](../README.md)重新执行现有 A/B 自动回归、Bash 安装器隔离单元测试及文档检查。无需 Mac 或人工操作业务界面。所有已执行阶段通过，客机已恢复基线，宿主路由、DNS 摘要和既有 Mihomo 服务状态前后相同。

## 构建与环境

- 运行开始：2026-10-02 14:06:52 UTC。
- ClashTui：`0.3.2-3637c25-dirty`，Rust 1.95.0；保留当前未提交工作树，未修改业务代码。
- 本轮宿主二进制 SHA-256：`a8b1e3cb3fc0015ea3f61c32e702c2056dd145e2c0d8957830eecc2089b96fc0`。
- 真实环境：Debian 12/KVM、Mihomo 1.19.24、固定 MetaCubeXD v1.273.1，归档 SHA-256 按固定值验证。
- 浏览器：mini PC 无头 Chromium 154；真实页面通过回环 SSH 隧道访问客机。订阅、流量和 DNS 目标均为客机受控本地目标。

## 结果与证据

| 层次 | 结果 | 本地证据 |
|---|---|---|
| 完整隔离流水线 | 9 阶段通过、0 失败；默认实机阶段跳过，由独立 VM 补充 | `target/automated-results/20261002T140652Z/isolated/report.json` |
| Rust 全特性 | 353 单元、7 进程测试通过，1 手工测试忽略 | 同目录 `rust-all-features.log` |
| Rust CLI 特性 | 146 单元、7 进程测试通过，1 手工测试忽略；与全特性覆盖重叠 | 同目录 `rust-cli-only.log` |
| DOM / CLI / PTY / 模拟浏览器 | 5 / 5 / 3 / 6 项通过，包含固定面板 | 同目录各阶段日志 |
| Bash 安装器隔离单元 | 22 通过、0 失败；没有实际安装到宿主 | `target/automated-results/20261002T140652Z/installer-units.log` |
| 真实 VM | 32 组通过、0 失败、0 已知失败、1 能力跳过 | `target/vm/results-20261002T140852Z/report.json` |
| 汇总与恢复 | 8 个调度/检查阶段通过，含最终恢复与宿主对照 | `target/automated-results/20261002T140652Z/report.json` |

原始 VM 日志与报告另复制到汇总目录的 `real-vm-results/`，浏览器证据复制到 `browser/`，避免后续重跑覆盖本轮截图。旧客机夹具和证据在 reset 前备份；本轮客机日志在最终恢复前保存。证据可能含测试配置，分享前脱敏。

真实 VM 的 32 组包括：18 组共享工作流/缺陷回归、7 组客机网络/恢复、5 组真实页面操作、2 组真实 TUI。五项先前实机缺陷均再次通过；具体行为见[此前修复报告](vm_acceptance_20261002_zh.md)。

## 恢复核对

`guest-restored.json` 确认当前 Profile 为 baseline，两客机服务 active，TUN/DNS false，外连阻断，使用基线 overlay，面板索引摘要符合既有基线。客机保持运行，方便后续自动测试。

宿主 `routes-before/after.json`、`dns-before/after.txt`、`existing-service-before/after.txt` 的 diff 均为空；未修改宿主代理、路由、DNS 或服务。对照日志是 `host-invariants.log`。

本轮使用临时调度脚本执行取证与恢复，没有将它交付为仓库稳定总入口；方案中统一入口、失败/中断恢复包装仍需开发和失败注入验证。成功完成本次恢复不证明所有异常下都能自动恢复。

## 跳过与尚未覆盖

- Mihomo 1.19.24 不暴露规则 disabled 状态，规则开关能力跳过。
- Rust 中的人工交互用例忽略；H01–H06 未执行，也没有 Mac 原生或 Windows 验收。
- 方案 C 层的订阅错误矩阵、批次选项、执行中任务中断、停服迁移、更多真实页面/PTY 路径等仍待补自动断言。现有测试通过不等于这些子项已覆盖。
- 公网订阅、真实远程代理、联网升级、其他核心版本、规模和长时间运行未执行。
- 本轮使用 Rust 1.95.0；Rust 1.89 最低版本检查保留此前证据，没有本轮重跑；远端三平台 CI 仍无新结果。

本轮完成的是当前可执行自动回归，不宣称所有功能和平台已完备验收。下一步优先补齐[方案](../README.md)的 C 层自动断言，继续在 mini PC 上闭环。
