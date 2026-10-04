# ClashTui

<p>
  <a href="https://github.com/JohanChane/clashtui/releases"><img src="https://img.shields.io/github/v/release/JohanChane/clashtui" alt="Release"></a>
  <a href="https://github.com/JohanChane/clashtui/releases"><img src="https://img.shields.io/github/v/release/JohanChane/clashtui?include_prereleases&label=pre-release" alt="Pre-release"></a>
  <a href="https://github.com/JohanChane/clashtui/blob/main/LICENSE"><img src="https://img.shields.io/github/license/JohanChane/clashtui" alt="License"></a>
  <a href="https://github.com/JohanChane/clashtui/actions/workflows/build.yml"><img src="https://github.com/JohanChane/clashtui/actions/workflows/build.yml/badge.svg" alt="Build"></a>
  <a href="https://github.com/JohanChane/clashtui/actions/workflows/pr.yml"><img src="https://github.com/JohanChane/clashtui/actions/workflows/pr.yml/badge.svg" alt="PR"></a>
  <a href="https://github.com/JohanChane/clashtui/actions/workflows/release.yml"><img src="https://github.com/JohanChane/clashtui/actions/workflows/release.yml/badge.svg" alt="Release CI"></a>
</p>

<video src="https://github.com/user-attachments/assets/7808534a-84bc-4967-a024-534487ab7aaf" controls width="100%"></video>

Language: [English](README.md) | [中文](README_ZH.md)

ClashTui 是一个终端用户界面（TUI）代理管理工具，管理 **Mihomo**（Clash.Meta）代理核心。你可以在终端里完成切换节点、更新订阅、管理连接和控制服务启停等操作。

## 特性

- **Mihomo 管理** — CLI、TUI 和 Web 共用 Mihomo 管理能力
- **单核心迁移** — 旧配置兼容及数据备份见[迁移说明](docs/reference/mihomo_only_migration_zh.md)
- **订阅管理** — 支持 File、URL、Template (ClashTui 的 template) 三种 profiles。
- **代理切换** — 按组/按节点切换，支持延迟测试
- **连接监控** — 实时查看所有活动连接，可关闭单个或全部连接
- **服务控制** — 通过 systemd 管理核心的启动、停止和重启
- **日志查看** — 在界面内实时查看核心日志
- **命令行模式** — 支持 `profile`、`mode`、`service`、`update` 等子命令，适合脚本和自动化
- **配置覆盖** — 通过 `core_override_config` 在不修改订阅原始文件的前提下改写最终配置
- **Template 模板** — 用模板 + 节点分组自动生成配置文件，支持变量展开
- **自定义按键** — 每个标签页的快捷键都可以通过 `keymap.yaml` 自行定义
- **自定义主题** - 通过 `theme.yaml` 自行定义
- **which panel** - 方便用户操作
- **支持 fzf** - 用户可以使用 fzf 进行选择

## 支持的平台

-   [x] Linux
-   [x] macOs
-   [x] Windows

## 安装

### 依赖

Linux and macOS:
-   sudo
-   fzf

Windows:
-   nssm

### 想开启 tun 并有 root 权限

#### Linux

1. \[可选\] 从仓库中安装 mihomo 和 clashtui:

```sh
sudo pacman -S mihomo clashtui  # ArchLinux
```

这一步的目的是保证当前环境中包含 mihomo 和 clashtui，这样安装脚本会跳过安装它们的步骤。你也可以手动下载这些工具，然后运行 which mihomo clashtui 来检查是否已正确配置。

2. 运行安装脚本

```sh
bash <(curl -fsSL https://raw.githubusercontent.com/JohanChane/clashtui/refs/heads/main/installs/install) --core mihomo
```

提示：由于安装脚本使用的资源是从 GitHub 上下载的，所以如果总是下载失败，可以先开启代理再运行脚本。

3. \[可选\] 将 `clashtui_mihomo.service` 设置为开机启动

```sh
sudo systemctl enable clashtui_mihomo.service
```

#### macOS

1. \[可选\] 从仓库中安装 mihomo 和 clashtui:

```sh
brew install mihomo # 目前 clashtui 还没有上传, 请手动安装 clashtui
```

2. 运行安装脚本

和 Linux 一样

3. \[可选\] 将 `clashtui_mihomo.service` 设置为开机启动

```sh
sudo launchctl load -w /Library/LaunchDaemons/clashtui_mihomo.plist
```

#### Windows

1. \[可选\] 从仓库中安装 mihomo 和 clashtui:

```powershell
scoop install mihomo clashtui
# 验证
Get-Command mihomo clashtui
```

2. 运行安装脚本

```powershell
# 默认安装到 D:\ClashTui
irm https://raw.githubusercontent.com/JohanChane/clashtui/refs/heads/main/installs/install.ps1 | iex

# 安装到自定义目录 (路径不能有空格)
iex "& {$(irm https://raw.githubusercontent.com/JohanChane/clashtui/refs/heads/main/installs/install.ps1)} -Core mihomo -InstallDir 'D:\MyTools\ClashTui'"

# 只安装 mihomo core
iex "& {$(irm https://raw.githubusercontent.com/JohanChane/clashtui/refs/heads/main/installs/install.ps1)} -Core mihomo"
```

3. 启动 clashtui 安装 clashtui_mihomo 服务

安装脚本不注册 Windows Service。启动 clashtui 后，使用 CoreSrvCtl 安装并启动 core 服务。
按需将 Mihomo 服务设置为开机启动。

### 没有 root 权限 (不开启 tun)

#### Linux

```sh
bash <(curl -fsSL https://raw.githubusercontent.com/JohanChane/clashtui/refs/heads/main/installs/install) --core mihomo --is-user
```

开机启动:

```sh
systemctl --user enable clashtui_mihomo.service
```

#### macOS

```sh
bash <(curl -fsSL https://raw.githubusercontent.com/JohanChane/clashtui/refs/heads/main/installs/install) --core mihomo --is-user
```

开机启动:

```sh
launchctl load -w ~/Library/LaunchAgents/clashtui_mihomo.plist
```

## FAQ

- 文件权限之类的问题: 因为 clashtui 需要用到组权限, 所以请重新登录使组权限生效。
- connection refused 的问题: 比如: mihomo, 请使用 `netstat -utapln | grep 9090` 检查 clash api 端口有没有打开。

## 内置 Web 面板

Vue 3 + TypeScript + Vite 面板统一管理核心与本地配置、订阅、模板和服务。构建与认证见 [Web 面板文档](docs/development/web_dashboard_zh.md)，源代码、测试与脚本位置见[仓库导航](docs/development/architecture_zh.md#仓库导航)。

## 文档

先看[文档总入口](docs/README.md)，按用途查找；旧规划和历次审查统一收进[历史归档](docs/archive/README.md)。

| 常用入口 | 内容 |
|---|---|
| [当前项目状态](docs/reference/project_status_zh.md) | 当前范围、已测结果、未测项和三端差异 |
| [使用指南](docs/guides/getting_started_zh.md) | TUI、CLI、订阅与配置 |
| [自动闭环测试方案](docs/testing/README.md) | mini PC 自动回归与真实 VM；Mac 仅按需做 6 项体验检查 |
| [自动测试流水线](docs/testing/test_pipeline_zh.md) | 自动化命令与覆盖范围；[VM 操作](docs/testing/local_vm_testing_zh.md) |
| [开发文档](docs/development/architecture_zh.md) | 架构与模块；[功能设计](docs/development/clashtui_feature_design_zh.md) |

## 参与开发

欢迎提交 Issue 和 Pull Request。参与开发前请先阅读[开发约定](docs/development/development_conventions.md)。

快速了解项目, 加入开发:
1. [功能设计](docs/development/clashtui_feature_design_zh.md) — 了解功能设计
2. [架构](docs/development/architecture_zh.md) — 了解代码结构
