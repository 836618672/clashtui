# ClashTui 的功能设计

当前仅支持 Mihomo（Clash.Meta）。CLI、TUI 和 Web 共用 Rust 的配置、订阅、模板、激活及服务管理逻辑。

## 文件与配置

```
<配置目录>/
├── config.yaml
├── clashtui.db
├── clashtui.log
├── keymap.yaml                  # 可选
├── theme.yaml                   # 可选
└── mihomo/
    ├── core_override_config.yaml
    ├── profiles/               # 原始订阅/导入配置
    ├── templates/
    └── template_proxy_providers.yaml  # 旧模板分组格式，仍可读取
```

主配置的 `mihomo.core` 指定核心二进制、核心配置目录和有效配置文件；`mihomo.core_service` 指定服务名称、用户模式和 Linux 服务控制器。管理程序的数据目录与核心运行目录可以不同。

数据库仅保存 Mihomo 当前 Profile 和 Profile 元数据。旧数据库包含其他核心时，在转换前保存 `clashtui.db.before-mihomo-only`；保留原 Mihomo Profile，忽略已移除核心的记录。程序不会卸载电脑上已存在的其他核心或删除其配置目录。

## Profile 与模板

支持 File、URL、Template 三种 Profile。更新 URL 订阅会下载并检查 YAML 结构；原始文件保留在 profiles 中。订阅名称、URL、当前选择、Provider 内联和代理更新选项通过共享业务函数管理。

模板从 `mihomo/templates/` 读取，使用 `clashtui.proxy_provider_groups` 和 `${PPG.<group>}` 等占位符展开 Provider 与代理组。生成预览不注册 Profile；实际生成先调用 Mihomo 校验，再原子替换文件与保存元数据。不能覆盖其他类型或来自另一模板的 Profile。

覆盖配置按 Mihomo 顶层字段合并，替换指定字段；不使用其他核心的递归 JSON 合并规则。临时运行设置通过核心 API 修改；持久化是显式操作，写入覆盖配置供下次激活使用。

## 激活、同步与并发

激活流程：读取原始配置 → 合并覆盖 → 校验控制地址和密钥 → `mihomo -t` → 原子提交 → 核心 API 重载 → 有界回读验证。失败时尝试恢复旧配置，并核实恢复结果。

核心回读可以观察运行设置、节点/代理组、规则等信息，无法证明节点凭据或完整 Provider 内容。在线激活不能改变控制地址或密钥；迁移步骤见[测试指南](../testing/tui_web_testing_zh.md)。

业务写入使用跨进程锁。Web/CLI 内置文档编辑要求内容修订，旧修订不能覆盖新内容。TUI 外部编辑器不参与修订协议，应关闭后再更新或激活。

## 核心 API 与服务

HTTP/WebSocket 请求使用不可变的端点和认证快照。修改前验证 Mihomo 身份，认证失败、未知核心、接口不支持和 HTTP 失败不会当作成功。连接关闭按确认时捕获的 ID 集合执行。

Linux 支持 systemd/OpenRC，macOS 使用 launchd，Windows 使用 NSSM；Windows 的服务注册和系统代理按平台提供。远程核心端点不能控制本机服务或激活本机文件。核心类型选择和跨核心服务切换已移除。

## 用户入口

TUI 提供 Status、Files、Proxies、Connections、Logs、Settings、Service、Rules、Providers 九个标签。自定义快捷键和主题可以保留 `mihomo` 覆盖段。

Web 当前由锁定版本 MetaCubeXD 核心面板与本地管理页组成。原版面板显示核心数据；管理页管理本地订阅、模板、覆盖配置和服务。MetaCubeXD 源码整合尚未实施。

详细入口见[三端功能矩阵](../reference/cli_tui_web_parity_zh.md)，架构见[架构说明](architecture_zh.md)。

## 开发与验证

格式化、严格 Clippy、两种 Rust 特性组合、隔离进程、Web DOM、TUI 伪终端及浏览器测试由[测试流水线](../testing/test_pipeline_zh.md)统一执行。默认只使用临时目录、服务桩和回环模拟 API。真实网络、TUN、服务安装与上游面板联调在隔离 VM 中验收。
