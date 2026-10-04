# 本地虚拟机测试环境

默认按[mini PC 自动闭环方案](README.md)在本机执行真实核心、无头浏览器与 PTY 测试，无需图形界面或 Mac。只有六项人工体验检查按需使用另一台 Mac，连接与排障参考[Mac 接入指南](tui_web_testing_zh.md)，结果填[简化记录](manual_test_record_zh.md)。Mac 远程操作不计作 macOS 原生服务验收。

2026-10-02 后续已执行真实核心、三端和客机 DNS/TUN 测试，五项缺陷已修复，结果及能力边界见[验收记录](reports/vm_acceptance_20261002_zh.md)。可用 `bash scripts/vm-test.sh` 从基线重跑；该入口会丢弃客机本轮修改并在失败时退出非零。

2026-10-02 已在本机实际搭建并启动：Debian 12、KVM、Mihomo 1.19.24、当前 ClashTui 构建和 MetaCubeXD v1.273.1。真实核心校验、服务重启、配置激活、管理页认证、面板 HTTP 访问及基线恢复启动已通过。恢复后两个客机服务运行，默认外连阻断、TUN/DNS 关闭；宿主路由和 `/etc/resolv.conf` 摘要与启动前一致。证据在 `target/vm/acceptance-smoke.json`，可以通过 `bash scripts/vm.sh check` 重新验证默认离线环境。后续完整功能、TUN 与网络故障测试已执行，详细范围见验收记录。

环境使用 Debian 12 官方 genericcloud 镜像、QEMU 和 cloud-init。所有镜像、密钥、日志和运行记录放在被 Git 忽略的 `target/vm/`。基础镜像按官方 HTTPS 提供的 SHA-512 清单校验，实际摘要与来源保存在 `image.json`。

虚拟机分配 2 个 CPU、1536 MiB 内存、12 GiB 稀疏磁盘。可以访问 `/dev/kvm` 时使用 KVM，否则使用 TCG。宿主不安装代理服务，不创建 TAP/网桥，不修改路由、DNS 或系统代理，不共享宿主目录、磁盘或配置。默认 `restrict=on`，阻止客机主动外连；SSH 通过 QEMU 的回环端口转发进入客机，Web/API 通过 SSH 隧道访问。

## 启动和访问

NixOS 上脚本自动用 `nix shell` 获取 QEMU、cdrkit、SSH、curl、Python 和 binutils；首次获取工具和镜像需要网络。其他 Linux 可在 PATH 中准备这些程序。命令均从项目根目录运行：

```sh
bash scripts/vm.sh prepare
bash scripts/vm.sh start
bash scripts/vm.sh wait
bash scripts/vm.sh tunnel
bash scripts/vm.sh check
bash scripts/vm.sh ssh
```

固定端口：宿主 `127.0.0.1:22222` 是客机 SSH；`http://127.0.0.1:29090/` 是客机核心 API，`http://127.0.0.1:29091/` 是统一 Vue 面板。没有向宿主转发代理端口 7890。使用前脚本检查 SSH 端口占用，隧道创建也检查绑定失败。控制端口只绑定宿主回环地址。

## 传入构建和配置测试服务

先构建当前代码，然后传入二进制。Nix 构建所需的运行时库以独立副本传入，客机不访问宿主 Nix store。传入核心时使用指定文件的副本，不启动或修改该核心在宿主的服务。

```sh
cargo build --locked --all-features
bash scripts/vm.sh push --mihomo /path/to/mihomo
bash scripts/vm.sh ssh 'bash /opt/clashtui/setup.sh'
bash scripts/vm.sh tunnel
```

Vue 面板已包含在 ClashTui 中，无需面板归档。初始化脚本只允许在名为 `clashtui-test` 的客机中由 `tester` 用户运行，并拒绝覆盖已存在的测试配置。

客机有两个独立 systemd 服务：`clashtui-test-mihomo`、`clashtui-test-web`。测试配置位于 `/home/tester/clashtui-test/config`；默认 TUN、DNS、LAN 访问和 Geo 自动更新均关闭。核心 API 只监听客机回环 9090，本地管理页只监听客机回环 9091。管理令牌和核心密钥在客机内分别生成，不复制宿主密钥。

进入客机后：

```sh
clashtui --config-dir=/home/tester/clashtui-test/config core status
clashtui --config-dir=/home/tester/clashtui-test/config
cat /home/tester/clashtui-test/config/management-token
```

将最后一条显示的测试令牌填入统一 Vue 页面。核心凭据由 Rust 读取，浏览器无需填写。

## 快照、恢复和关闭

配置完成后正常关闭并建立基线：

```sh
bash scripts/vm.sh stop
bash scripts/vm.sh checkpoint
bash scripts/vm.sh start
bash scripts/vm.sh wait
bash scripts/vm.sh tunnel
```

每轮测试后的恢复操作会丢弃客机磁盘上的本轮修改，保留基线。先把需要的日志从客机导出，再执行：

```sh
bash scripts/vm.sh stop
bash scripts/vm.sh reset
bash scripts/vm.sh start
bash scripts/vm.sh wait
bash scripts/vm.sh tunnel
```

checkpoint/reset 都拒绝操作正在运行的磁盘；已存在的基线不会被覆盖。`stop` 使用 ACPI 正常关机，超时保留运行状态并报错，不强制破坏磁盘。`status` 查看状态，`target/vm/console.log` 查看串口启动日志。SSH 隧道随客机关机断开。

## 测试范围

默认禁止外连的环境适合真实核心校验、CLI/TUI/管理页入口、面板加载、服务启停与重启、配置激活/恢复、文件事务、Provider 回环订阅及失败注入。此前[Provider 审查](../archive/README.md#providers)中的四项问题已修复并加入 VM 回归；失败注入只使用客机测试文件。

真实公网代理、订阅下载或更新需要关闭客机后显式执行 `start --internet`，这会允许客机通过用户态 NAT 外连，包括访问可路由的宿主/LAN 地址；没有设置网桥或更改宿主默认路由。完成联网用例后恢复默认启动。涉及 TUN/DNS 的测试只在客机中进行，完成后恢复基线；本机环境不覆盖 macOS/Windows 验收。

搭建、启动成功不代表完整验收通过。按[测试方案](README.md)分别记录自动、待补、人工体验和平台专项；原 M 用例仅按需复现。

可用 `CLASHTUI_VM_DIR=target/vue-vm` 与 `CLASHTUI_VM_SSH_PORT=22223`、`CLASHTUI_VM_CORE_PORT=29190`、`CLASHTUI_VM_WEB_PORT=29191` 建立第二套隔离环境，避免重置正在进行 Mac 人工测试的原 VM。目录必须位于项目 target 内，端口互异；浏览器脚本读取同一组环境变量。
