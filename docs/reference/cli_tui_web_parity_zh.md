# CLI、TUI 与 Web 功能对齐记录

更新：2026-10-03。本文是当前三端业务入口参考，不是所有交互均已通过验收的证明。当前状态和未测项见[项目状态](project_status_zh.md)，本次实机证据见[验收报告](../testing/reports/vm_acceptance_20261002_zh.md)。

Web 指 **ClashTui 内置 Vue 面板**，核心和本地管理由同一个 Rust 服务提供。快捷键、鼠标、终端布局和浏览器图表保留各自交互方式。

## 核对结论

以下矩阵说明业务入口对应。跨端事务、资源缓存、启动验证、激活证据与任务恢复已补齐，当前交付范围进入人工验收；实现范围、验证结果及接口限制见[项目状态](project_status_zh.md)。

原 CLI 只有 Profile 列表/更新/选择、模式和少量服务命令，与 TUI 不对齐。本轮新增 `manage` 和 `core` 命令族，补齐下表的业务入口；原命令继续可用。Web 本地管理页补齐批量更新、配额查询、URL 元数据、配置预览/校验、模板删除与 Provider 分组、生成预览及服务管理。

| 业务能力 | CLI | TUI | Web |
|---|---|---|---|
| 核心身份、状态、运行配置 | core status / manage runtime | Status / Settings | Vue 总览 / 设置 |
| 节点列表、选择、节点/组测速 | core proxies / select / delay | Proxies | Vue 面板 |
| 恢复自动选组、Provider 内单节点测速 | core unfix GROUP / delay NODE --provider PROVIDER | Proxies u / Providers d | Vue 面板 |
| 规则、代理/规则 Provider 列表与详情 | core resources … list | Rules / Providers | Vue 面板 |
| Provider 单项/批量更新、健康检查、规则停用 | core resources … update/update-all/health | Rules / Providers | Vue 面板 |
| 连接详情、字段过滤、JSON 输出、固定集合关闭 | core connections --filter / --id / --close | Connections | Vue 面板 |
| 日志筛选、有限采集、导出 | core logs --level/--filter/--limit/--seconds/--output | Logs | Vue 面板 |
| 速率、会话量、连接数、有限历史、可用内存 | core metrics --samples/--interval-ms | Status | Vue 面板 |
| 自定义运行模式 | mode set VALUE | Settings 根据 mode-list 显示 | Vue 设置（依据 mode-list） |
| 临时设置、持久覆盖配置 | manage patch / persist | Settings e / p | 管理页 |
| DNS/fake-IP 清理、GEO 更新、API 重启/升级 | core maintenance | Settings / Service | Vue 面板 |
| Profile 列表、导入、新订阅、编辑名称/URL、删除 | profile list / manage import/create/rename/delete | Files | 管理页 |
| 文件读取、版本化编辑、预览、导出 | manage read/save/preview；stdout 重定向 | Files 预览和外部编辑器；连接/日志/资源专用导出 | 管理页编辑/预览/导出 |
| 更新、批量更新、激活 | profile update/select；manage update/update_all/activate | Files；默认 U 批量更新 | 管理页 |
| 独立配置校验/测试 | manage check/test | Files | 管理页 |
| URL 读取/复制、流量额度 | manage profile_url/traffic（JSON） | Files 复制/额度 | 管理页复制/额度 |
| 内联 Provider、代理更新选项 | manage no_pp/with_proxy | Files | 管理页 |
| 模板列表、创建、编辑、删除 | manage state/read/save/delete_template | Files；a 新建 | 管理页 |
| 模板 Provider 分组读取/编辑 | manage template_providers/save_template_providers | 模板 E 编辑元数据 | 管理页专用 JSON 编辑 |
| 生成预览、生成配置 | manage preview_template/generate | 模板 P / Enter | 管理页 |
| 服务状态、启动/停止/重启 | service status/start/stop/restart | Service | 管理页 |
| Windows 服务注册与系统代理 | service install/uninstall/system-proxy | Service | 管理页按平台显示 |
| 内置面板可用性 | panel / manage prepare_panel（不下载） | Status i 检查 / w 打开 | 页面内置，管理令牌认证 |

项目仅管理 Mihomo。核心版本不提供的 API/字段不承诺可用；配置修改仅开放已知可写且存在的字段。

## 命令示例

示例省略测试配置目录；实际测试应统一添加 `--config-dir=/path/to/test-config`。以下是供人工测试使用的命令，必须在测试环境执行。

```sh
clashtui manage state
clashtui manage create --name demo --url https://example.com/sub
clashtui manage import --name local --input profile.yaml
clashtui manage rename --name demo --new-name demo2 --url https://example.com/new-sub
clashtui manage traffic --name demo2
clashtui manage profile_url --name demo2
clashtui manage check --name demo2
clashtui manage preview --name demo2
clashtui manage update_all --yes
clashtui manage activate --name demo2 --yes

# read 返回 content 与 revision；保存必须传入读取时的 revision。
clashtui manage read --kind profile --name demo2
clashtui manage save --kind profile --name demo2 --input edited.yaml --revision REVISION
# 新模板先读取空文档，保存时使用返回的 revision。
clashtui manage read --kind template --name custom.yaml
clashtui manage save --kind template --name custom.yaml --input template.yaml --revision REVISION
clashtui manage template_providers --name custom.yaml
clashtui manage save_template_providers --name custom.yaml --input groups.json --revision REVISION
clashtui manage preview_template --name custom.yaml
clashtui manage generate --name custom-generated --template custom.yaml
clashtui manage delete_template --name unused.yaml --revision REVISION --yes

clashtui manage runtime
clashtui manage patch --input patch.json
clashtui manage persist --yes
clashtui mode set custom-mode
clashtui mode rule --close-connections
clashtui core status
clashtui core proxies
clashtui core select GROUP NODE
clashtui core unfix GROUP
clashtui core delay NODE --provider PROVIDER
clashtui core delay GROUP --group --timeout 5000
clashtui core resources rules list
clashtui core resources rules update --name 10 --yes
clashtui core resources proxy-providers update-all --yes
clashtui core resources proxy-providers health --name PROVIDER --yes
clashtui core connections --filter worker.exe
clashtui core connections --filter worker.exe --close --yes
clashtui core logs --level info --filter timeout --seconds 10 --limit 100 --output logs.json
clashtui core metrics --samples 30 --interval-ms 1000
clashtui core maintenance flush-dns --yes
clashtui service status
```

`manage` 的动作名称使用下划线；`core`/`service` 子命令使用连字符。`manage --with-proxy true/false` 可覆盖保存选项；原 `profile update --with-proxy` 保持可用，`--with-proxy=false` 显式关闭。未指定时沿用 Profile 的保存值。新建 URL 订阅先下载并校验结构，再注册。

`save`、Provider 分组保存和模板删除要求文档修订。三端拒绝删除当前 Profile；删除仍被 Profile 引用的模板也会被拒绝。生成配置不能覆盖其他类型或另一模板名下的 Profile；CLI 重生成已存在配置需要 `--yes`，Web 显式确认。

TUI 模板生成同样允许输入 Profile 名称，覆盖已有生成配置需确认。服务启动/重启只有通过有界身份与配置读取检查后才报告核心就绪；就绪失败不会自动重试服务操作。Test/Check 均保留 valid、exit_code、stdout/stderr 和 activated=false；CLI 校验失败以非零状态退出，Web/TUI 展示诊断。

TUI Test/Check 在后台使用共享管理动作，即使 Profile 已被外部编辑为非法 YAML，也将原文件交给核心校验器，返回与 CLI/Web 相同的诊断结构。普通节点、组和 Provider 单节点测速的 `timeout` 单位为毫秒，允许 1～3,600,000；HTTP 等待按 `max(20000, timeout + 10000)` 毫秒向上取整为秒，身份检查仍使用全局超时。测速 URL 的已有百分号编码在服务端解析后完整保留。

## 有意保留的交互差异

- TUI Profile 默认 `U` 批量更新，并逐项报告混合失败；与 CLI/Web 使用同一管理动作。
- CLI 输出有限快照/采样 JSON，日志默认最多 100 条、10 秒；TUI/Web 提供持续展示和暂停。CLI 不维护跨进程历史或浏览器图表。
- Unix TUI 的终端编辑器通过配置 `edit_cmd: vi %s` 接管终端并在退出后恢复；非零退出明确报错。TUI 文件编辑使用外部编辑器，无法给不参与锁协议的编辑器提供与 Web/CLI 修订保存相同的并发保障。Web 和 CLI `manage save` 都检查内容修订。
- 复制 URL 在 CLI 表现为可管道输出；TUI/浏览器使用各自平台剪贴板。TUI 自定义主题、按键与 fzf 不需要复制到 CLI。
- 原 CLI 自更新命令属于发布工具入口，未新增浏览器远程替换 ClashTui 可执行文件功能。上游 agent 的 WebDAV、script/merge 和桌面壳依然不在本次范围。

## 测试边界

Rust 单元与模拟 HTTP/WebSocket、CLI 参数解析，以及 Web 模拟 DOM/fetch 回归覆盖新入口。Web 测试可执行 `node --test web/index.test.cjs`。Chromium 管理页已与实际管理服务和模拟核心联调；独立 Linux VM 的真实核心、页面和 TUI 验证结果见[验收报告](../testing/reports/vm_acceptance_20261002_zh.md)。另一台 Mac 的手工交互、原生平台和公网测试仍需按[测试指南](../testing/tui_web_testing_zh.md)执行。
