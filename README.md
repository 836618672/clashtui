# ClashTui

<p>
  <a href="https://github.com/JohanChane/clashtui/releases"><img src="https://img.shields.io/github/v/release/JohanChane/clashtui" alt="Release"></a>
  <a href="https://github.com/JohanChane/clashtui/releases"><img src="https://img.shields.io/github/v/release/JohanChane/clashtui?include_prereleases&label=pre-release" alt="Pre-release"></a>
  <a href="https://github.com/JohanChane/clashtui/blob/main/LICENSE"><img src="https://img.shields.io/github/license/JohanChane/clashtui" alt="License"></a>
  <a href="https://github.com/JohanChane/clashtui/actions/workflows/build.yml"><img src="https://github.com/JohanChane/clashtui/actions/workflows/build.yml/badge.svg" alt="Build"></a>
  <a href="https://github.com/JohanChane/clashtui/actions/workflows/pr.yml"><img src="https://github.com/JohanChane/clashtui/actions/workflows/pr.yml/badge.svg" alt="PR"></a>
  <a href="https://github.com/JohanChane/clashtui/actions/workflows/release.yml"><img src="https://github.com/JohanChane/clashtui/actions/workflows/release.yml/badge.svg" alt="Release CI"></a>
</p>

[clashtui_demo.webm](https://github.com/user-attachments/assets/51d3ee85-b1f4-4d02-9a43-623153e3825b)

Language: [English](README.md) | [中文](README_ZH.md)

ClashTui is a terminal user interface (TUI) proxy management tool supporting the **Mihomo** (Clash.Meta) proxy core. Switch nodes, update subscriptions, manage connections, and control services — all from the terminal.

## Features

- **Mihomo Management** — Manage the Mihomo core from the CLI, TUI and Web
- **Legacy Data Migration** — Existing Mihomo profiles are retained and legacy databases are backed up before rewriting; see the [migration notes (Chinese)](docs/reference/mihomo_only_migration_zh.md)
- **Subscription Management** — Supports File, URL, and Template (ClashTui template) profile types
- **Proxy Switching** — Switch by group or by node, with latency testing
- **Connection Monitoring** — View all active connections in real time; close individual or all connections
- **Service Control** — Manage core start, stop, and restart via systemd
- **Log Viewing** — View core logs in real time within the interface
- **CLI Mode** — Supports `profile`, `mode`, `service`, `update` subcommands for scripting and automation
- **Config Override** — Override final config via `core_override_config` without modifying original subscription files
- **Template System** — Auto-generate config files using templates + proxy node groups, with variable expansion
- **Custom Key Bindings** — Customize shortcuts for each tab via `keymap.yaml`
- **Custom themes** - user-definable via theme.yaml
- **which panel** - convenient for user operations
- **Supports fzf** - users can use fzf for selection

## Supported Platforms

-   [x] Linux
-   [x] macOs
-   [x] Windows

## Installation

### Requirements

Linux and macOS:
-   sudo
-   fzf

Windows:
-   nssm

### With root access (for TUN mode)

#### Linux

1. \[Optional\] Install mihomo and clashtui from your package repository:

```sh
sudo pacman -S mihomo clashtui  # ArchLinux
```

This step ensures mihomo and clashtui are available in your environment so the install script will skip downloading them. You can also download them manually and run `which mihomo clashtui` to verify they are correctly configured.

2. Run the install script:

```sh
bash <(curl -fsSL https://raw.githubusercontent.com/JohanChane/clashtui/refs/heads/main/installs/install) --core mihomo
```

Tip: The install script downloads resources from GitHub. If downloads keep failing, try enabling a proxy before running the script.

3. \[Optional\] Enable `clashtui_mihomo.service` on boot:

```sh
sudo systemctl enable clashtui_mihomo.service
```

#### macOS

1. \[Optional\] Install mihomo and clashtui from Homebrew:

```sh
brew install mihomo # Note: clashtui was NOT uploaded, please install it manually
```

2. Run the install script (same as Linux):

```sh
bash <(curl -fsSL https://raw.githubusercontent.com/JohanChane/clashtui/refs/heads/main/installs/install) --core mihomo
```

3. \[Optional\] Enable `clashtui_mihomo` launchd plists on boot:

```sh
sudo launchctl load -w /Library/LaunchDaemons/clashtui_mihomo.plist
```

#### Windows

1. \[Optional\] Install mihomo and clashtui from Scoop:

```powershell
scoop install mihomo clashtui
# Verify
Get-Command mihomo clashtui
```

This step ensures mihomo and clashtui are in PATH so the install script will skip downloading them.

2. Run the install script (as Administrator):

```powershell
# Default install to D:\ClashTui
irm https://raw.githubusercontent.com/JohanChane/clashtui/refs/heads/main/installs/install.ps1 | iex

# Custom directory (no spaces allowed)
iex "& {$(irm https://raw.githubusercontent.com/JohanChane/clashtui/refs/heads/main/installs/install.ps1)} -Core mihomo -InstallDir 'D:\MyTools\ClashTui'"

# Only install mihomo core
iex "& {$(irm https://raw.githubusercontent.com/JohanChane/clashtui/refs/heads/main/installs/install.ps1)} -Core mihomo"
```

3. Start clashtui, then use CoreSrvCtl to install and start core services:

The install script does NOT register Windows Services. Launch clashtui and use the built-in CoreSrvCtl to manage services. Enable the Mihomo service at boot if needed.

### Without root access (no TUN)

#### Linux

```sh
bash <(curl -fsSL https://raw.githubusercontent.com/JohanChane/clashtui/refs/heads/main/installs/install) --core mihomo --is-user
```

Enable on boot:

```sh
systemctl --user enable clashtui_mihomo.service
```

#### macOS

```sh
bash <(curl -fsSL https://raw.githubusercontent.com/JohanChane/clashtui/refs/heads/main/installs/install) --core mihomo --is-user
```

Enable on boot:

```sh
launchctl load -w ~/Library/LaunchAgents/clashtui_mihomo.plist
```

## FAQ

-   File permission issues: Since clashtui requires group permissions, please log in again for the group permissions to take effect.
-   Connection refused issues (e.g., with mihomo): Please use `netstat -utapln | grep 9090` to check whether the Clash API port is open.

## Built-in Web dashboard

Vue 3 + TypeScript + Vite provides core and local management in one embedded page. See the [dashboard build guide (Chinese)](docs/development/web_dashboard_zh.md) and [repository map](docs/development/architecture_en.md#repository-map).

## Documentation

Start with the [documentation index](docs/README.md). Current guides, testing instructions and historical records are grouped by purpose.

| Entry | Purpose |
|---|---|
| [Getting Started](docs/guides/getting_started_en.md) | TUI, CLI, subscriptions and configuration |
| [Manual Configuration](docs/guides/install_manually_en.md) | Prepare Mihomo and ClashTui configuration |
| [Current Status (Chinese)](docs/reference/project_status_zh.md) | Scope, completed verification and remaining acceptance |
| [Test Plan (Chinese)](docs/testing/README.md) | Automate on the headless mini PC; use a Mac only for six optional human experience checks |
| [Architecture](docs/development/architecture_en.md) | Code structure; [feature design](docs/development/clashtui_feature_design_en.md) |

[Automated testing](docs/testing/test_pipeline_zh.md), [VM operations](docs/testing/local_vm_testing_zh.md) and the [historical archive](docs/archive/README.md) are currently documented in Chinese.

## Contributing

Issues and pull requests are welcome. Please read the [Development Conventions](docs/development/development_conventions.md) before contributing.

To get up to speed quickly:
1. [Feature Design](docs/development/clashtui_feature_design_en.md) — understand the feature design
2. [Architecture](docs/development/architecture_en.md) — understand the code structure
