# Provider 工作流再次审查

> **历史归档**：本文保留当时的规划、发现和验证范围；正文中的“尚未修复/测试”等结论属于历史状态。当前状态见[项目状态](../reference/project_status_zh.md)，后续修复与实机证据见[2026-10-02 验收报告](../testing/reports/vm_acceptance_20261002_zh.md)。

当前状态：2026-10-02 四项 Provider 缺陷均已修复，并与只读 CLI 修订冲突一起通过实机回归。最终行为和证据见[修复验收](../testing/reports/vm_acceptance_20261002_zh.md)。以下保留修复前发现与复现记录。

历史日期：2026-10-01。本轮为代码审查与隔离复现，没有修改业务代码，没有安装或启动真实代理，没有修改系统网络。上一轮配置诊断、测速等待和 URL 编码三项修复仍有效；但新发现以下四项问题，尚未修复，不能据现有流水线宣称所有边界已闭环。

## P1：Provider 下载路径未限制在核心配置目录内

位置：`src/functions/file/template.rs:749`、`:777`，`src/functions/file/net_resource.rs:106`。

资源提取直接接受配置的 `path`；预下载通过 `config_dir.join(path)` 构造路径后原子写入。没有拒绝 `..`、绝对路径或检查祖先目录的符号链接。普通 Profile 更新和激活预下载均经过此函数；内联 Provider 另有直接拼接路径的缓存写入，也应一并检查。

隔离复现：在临时核心目录 `mihomo/` 的父目录创建 `sentinel.yaml`，内容为 `keep-existing-file`。导入包含 `proxies: []` 和 HTTP rule-provider 的配置，设置 `path: ../sentinel.yaml`；回环订阅返回合法 YAML。执行 `manage update` 返回 `partial_failure=false`，父目录文件被替换为 `payload: [example.test]`。没有接触宿主真实文件。

这使订阅配置可以借 Rust 预下载覆盖当前进程有权限写入的目录外文件。Mihomo 本身默认将 Provider 路径限定于 HomeDir，额外位置通过 SAFE_PATHS 指定，Rust 预下载绕过了这一限制。参见[官方规则集合文档](https://wiki.metacubex.one/config/rule-providers/)。

建议：所有 Provider 下载与缓存写入共用路径校验器，默认限定于核心配置目录，检查规范化后的目标和祖先符号链接；如果支持额外安全目录，应采用明确的授权目录配置。加入相对越界、绝对路径、祖先符号链接和正常子目录回归。

## P2：合法的 text/mrs 规则集被按 YAML Mapping 校验

位置：`src/functions/file/template.rs:763`、`src/functions/file/net_resource.rs:21`。

资源模型没有携带 rule-provider 的 `format`/`behavior`；预下载对代理和规则资源一律解析为 YAML Mapping。Mihomo 支持 yaml、text、mrs 格式，见[官方规则集合文档](https://wiki.metacubex.one/config/rule-providers/)。

隔离复现：导入 `format: text`、`behavior: domain` 的 HTTP rule-provider，回环服务器返回 `example.test` 和 `+.example.org` 两行。`manage update` 返回 `partial_failure=true`、`Invalid YAML format`，退出码 1，不写入合法规则文件。mrs 也会被同一校验逻辑拒绝，本轮只实际复现 text。

建议：提取并保留资源格式，按代理/规则、yaml/text/mrs 分别校验。内联规则功能需针对格式解析或明确报不支持，不能将非 YAML 格式统一视为下载损坏。

## P2：模板 Provider 下载失败且有缓存时报告更新成功

位置：`src/functions/file/profile.rs:406`。

模板 Profile 更新下载错误时，只要旧缓存可解析为 Mapping，就返回 `ok=true,error=null`，丢失本次网络错误。共享管理层因此返回 `partial_failure=false`，CLI 退出 0，Web/TUI 也无法展示这次失败。

隔离复现：生成含 Provider 分组的模板 Profile，预置合法缓存；回环订阅返回 HTTP 401。执行 `manage update` 输出资源 `ok=true,error=null`、`partial_failure=false`，退出码 0。旧缓存被保留，但没有成功更新。

建议：区分“本次更新成功”和“旧缓存可用”，保留失败原因并明确缓存回退状态。单次与批量更新的三端结果都应显示部分失败，而不是把缓存可用计为下载成功。

## P2：显式清空 Provider 分组后仍回退到旧分组

位置：`src/functions/file/template.rs:174`。

`read_template_ppg()` 只有在分组非空时接受模板内值；显式空对象与缺少该字段都回退到旧 `template_proxy_providers.yaml`。因此用户通过 Web/CLI 保存 `{}` 或通过 TUI 编辑清空后，旧分组再次生效，后续生成仍使用这些订阅。

隔离复现：模板包含 `clashtui.proxy_provider_groups: {}`，旧文件包含 `legacy.old` 指向回环订阅。`manage template_providers` 返回旧 `legacy` 分组，而不是 `{}`。

建议：仅字段缺失时执行兼容回退；`Some(empty)` 应表示明确清空。加入旧文件存在时保存、读回、预览和生成的回归。

## 验证范围

以上四项均以当前 `target/debug/clashtui` 二进制、临时配置目录、假校验程序及回环模拟订阅复现。真实核心未执行；mrs、符号链接路径属于源码确认的同路径风险，未另行运行实机验证。本轮未重复完整流水线；此前 9 阶段通过的记录仍有效，但缺少这些输入和失败路径。
