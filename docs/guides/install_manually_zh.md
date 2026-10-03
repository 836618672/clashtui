# ClashTui 手动配置指南（Mihomo）

当前只支持 Mihomo。真实安装、服务和 TUN 测试应在隔离 VM 中进行；安全的本地验证入口见[测试流水线](../testing/test_pipeline_zh.md)。

## 准备文件

准备与平台匹配的 ClashTui 和 Mihomo 二进制，或在源码目录执行 `cargo build --release --locked` 构建 ClashTui。将 Mihomo 放在固定路径，并准备独立的核心配置目录。

建议使用独立应用目录，例如 `~/.config/clashtui-test`，不要覆盖日常配置。复制 `contrib/default_configs/config.yaml`，修改 `mihomo.core` 中的三个路径为绝对路径。

```yaml
mihomo:
  core:
    bin_path: /absolute/path/to/mihomo
    config_dir: /absolute/path/to/core-config
    config_path: /absolute/path/to/core-config/config.yaml
  core_service:
    service_name: clashtui_mihomo
    is_user: true
    service_controller: systemd
timeout: 5
extra:
  edit_cmd:
  open_dir_cmd:
```

Windows 使用实际 mihomo.exe 路径，删除 service_controller；macOS 同样删除该字段以使用 launchd。Windows 服务操作使用 NSSM。创建服务名称需与主配置一致。

## 应用目录

创建 `mihomo/profiles` 和 `mihomo/templates`，将本地配置或订阅交给 Files 页或 CLI 导入。覆盖配置保存在 `mihomo/core_override_config.yaml`。

初次无 TUN 试用可参考仓库的 `contrib/default_configs/mihomo/core_override_config_no_tun.yaml`，核对控制 API、secret、代理端口和 DNS 设置。核心有效配置与应用覆盖配置中的控制地址和密钥应一致。需要测试网络连通性时，由测试命令显式指定代理地址。

```sh
clashtui --config-dir=/absolute/path/to/clashtui-test
clashtui --config-dir=/absolute/path/to/clashtui-test manage state
```

## 服务与权限

先在 VM 中校验有效配置，再注册系统服务：Linux 使用 systemd 或 OpenRC，macOS 使用 launchd，Windows 使用 NSSM。可参照仓库安装脚本的 Mihomo 服务生成函数；不要在日常电脑上执行系统安装验收。

Mihomo 服务需要读取有效配置并写入其缓存目录。systemd 用户服务在 `~/.config/systemd/user/`，系统服务的运行用户和组需与目录权限匹配。macOS 用户服务在 `~/Library/LaunchAgents/`；系统服务在 `/Library/LaunchDaemons/`。

```sh
clashtui service status
clashtui service start
clashtui service stop
clashtui service restart
```

这些命令操作实际服务，仅在已经配置好的测试机中执行。跨核心切换和其他核心的下载、模板与服务入口已移除。

## 模板、Web 与验收

模板可从 `contrib/templates/mihomo/` 复制到应用目录中的 `mihomo/templates/`。配置 Provider 分组后，在 Files 模板面板预览或生成。

Web 管理令牌与核心 secret 独立；启动方式、面板部署和控制地址迁移步骤见[Web 使用及测试指南](../testing/tui_web_testing_zh.md)。人工验收应记录核心版本、服务状态、有效配置和恢复结果。
