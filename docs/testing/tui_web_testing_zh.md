# Mac 按需接入与 M01–M30 详细排障参考

更新：2026-10-02。默认测试策略已改为[mini PC 自动闭环方案](README.md)：在 mini PC 上执行自动回归及待补断言，只有 H01–H06 体验检查需要人用 Mac。**本文不是必须逐项执行的 30 项人工清单**；保留原编号作为覆盖索引和失败后的详细复现参考。

接入 Mac 时，按第 2 节 VM 准备与第 3 节 SSH 连接，再做新方案第 5 节的六项检查；基础体验检查不需要启动本地订阅/DNS/流量夹具。原 M 用例仅在需要定位对应问题或专项复现时执行。结果填[简化人工记录](manual_test_record_zh.md)，已有自动结果见[修复验收记录](reports/vm_acceptance_20261002_zh.md)。

## 1. 测试部署与原则

当前 Linux 宿主没有图形界面。采用以下部署，Mac 只提供浏览器与终端：

```text
Mac：Chrome / Safari + Terminal
 ├─ 浏览器 127.0.0.1:39091 → SSH → Linux 127.0.0.1:29091 → SSH → VM 127.0.0.1:9091 管理页
 ├─ 浏览器 127.0.0.1:39090 → SSH → Linux 127.0.0.1:29090 → SSH → VM 127.0.0.1:9090 MetaCubeXD/API
 └─ 终端 SSH → Linux → 测试 VM：CLI / TUI / 离线订阅与流量夹具
```

本方案测试的是“**Linux 客机核心及业务层 + Mac 浏览器和远程终端**”。不代表测试了 Mac 原生 ClashTui、launchd 或 macOS 系统代理。Mac 无需安装 Mihomo、配置系统代理、启用 TUN 或修改 DNS。Linux 宿主既有 Mihomo 也不参与测试。

Web 由两个页面共同提供功能：

- **ClashTui 管理页**：订阅、文件、模板、覆盖配置、运行设置和服务管理。
- **MetaCubeXD v1.273.1**：节点、Provider、规则、连接、指标和日志。

所有 `127.0.0.1` 都属于**执行命令或打开页面的那台机器**。不要把 Mac 的浏览器地址填作客机订阅 URL；订阅请求由客机发出。

基本测试保持 VM 默认阻断外连。订阅、测速、DNS 上游和代理目标都用客机回环夹具。公网、升级及平台安装测试单独进行。不要同时运行 `scripts/vm-test.sh`；它会重置客机磁盘，破坏手工测试现场。

### 按需执行顺序

1. 首先在 mini PC 按[测试方案](README.md)运行 A/B 层，保留新报告并恢复基线。可自动化的缺口登记到 C 层，不安排用户逐项手工重跑。
2. 若需要体验验收，使用第 2 节 VM 准备、令牌与第 3 节 Mac 连接，执行新方案 H01–H06；只做代表路径。
3. 某项失败时选择本文对应 M 用例复现。夹具仅在用例需要时准备，不因接入 Mac 而全部启动。
4. M24～M28 的 DNS/TUN/公网/升级/迁移属于 mini PC 自动化或专项测试；M29 原生平台在相应系统以自动化为主。缺少自动脚本是待补项，不是必须手工的理由。
5. 结束执行第 11 节取证和恢复，故障查第 12 节。复现时不要并发运行会 reset 客机的 `vm-test.sh`。

### 1.1 固定地址与目录

| 用途 | 地址 / 目录 | 从哪里使用 |
|---|---|---|
| 项目 | `/home/yulinye/clashtui` | Linux 宿主 |
| 客机 SSH | Linux `127.0.0.1:22222` | 由 `scripts/vm.sh ssh` 访问，不在 LAN 上开放 |
| Mac 管理页 | `http://127.0.0.1:39091/` | Mac 浏览器 |
| Mac MetaCubeXD | `http://127.0.0.1:39090/ui/` | Mac 浏览器 |
| 核心配置根目录 | `/home/tester/clashtui-test/config` | 客机 |
| 客机核心 API / 管理服务 | `127.0.0.1:9090` / `127.0.0.1:9091` | 客机 |
| 客机显式代理 | `127.0.0.1:7890` | 仅客机流量夹具使用，不转发到 Mac |
| 手工夹具目录 | `/home/tester/clashtui-test/manual` | 客机 |
| 离线 HTTP / DNS 上游 | `127.0.0.1:18080` / UDP `127.0.0.1:18053` | 客机 |

## 2. Linux 宿主准备

本节所有命令在 **Linux 宿主**执行。可以先从 Mac SSH 登录 Linux 再执行；不要在 Mac 或客机中执行 VM 管理命令。

先在宿主保留只读网络记录，用于结束后对照；这些命令不改变网络或服务：

```bash
cd /home/yulinye/clashtui
mkdir -p target/manual-results
ip -j route > target/manual-results/host-routes-before.json
sha256sum /etc/resolv.conf > target/manual-results/host-dns-before.txt
systemctl is-active mihomo.service > target/manual-results/host-existing-service-before.txt
```

最后一项只读取当前宿主既有服务；不存在时记录“不存在”，不要为测试安装或启动它。测试期间正常 DHCP/网络切换可能改变记录，应说明原因，不把每一个系统变化都归因于项目。

```bash
cd /home/yulinye/clashtui
bash scripts/vm.sh status
```

当前已有 VM、基线、Mihomo 与面板。VM 未运行时：

```bash
bash scripts/vm.sh start
bash scripts/vm.sh wait --seconds 45
```

传入当前构建，然后重启**客机管理服务**，使它使用新的二进制：

```bash
# 若当前构建已经是待测版本，可跳过构建。NixOS 示例：
nix shell nixpkgs#cargo nixpkgs#rustc nixpkgs#gcc -c cargo build --offline --locked --all-features
bash scripts/vm.sh push
bash scripts/vm.sh ssh 'sudo systemctl restart clashtui-test-web'
bash scripts/vm.sh tunnel
bash scripts/vm.sh check
```

预期：`check` 报告 `current_profile=baseline`、两个服务 `active`、`tun_enabled=false`、`dns_enabled=false`、`outbound_tcp_blocked=true`。该检查要求干净基线；后面切换 Profile 后不再用它判断日常操作是否成功。

如需从干净基线重新开始，先导出旧测试记录，再执行下列命令。**reset 会丢弃当前客机修改，基线还可能包含较早的 ClashTui 构建，因此 reset 后必须再次 push。**

```bash
bash scripts/vm.sh stop
bash scripts/vm.sh reset
bash scripts/vm.sh start
bash scripts/vm.sh wait --seconds 45
bash scripts/vm.sh push
bash scripts/vm.sh ssh 'sudo chgrp -R tester /home/tester/clashtui-test/config/mihomo; chmod g+s /home/tester/clashtui-test/config/mihomo; chmod -R g+w /home/tester/clashtui-test/config/mihomo; sudo systemctl restart clashtui-test-web'
bash scripts/vm.sh tunnel
bash scripts/vm.sh check
```

观察：客机为 `clashtui-test`；服务名只能是 `clashtui-test-mihomo`、`clashtui-test-web`。不要操作宿主的 `mihomo.service`。当前基线初始化已安装客机测试服务，不必重跑 `vm-guest-setup.sh`；它会拒绝覆盖现有配置。

### 2.1 准备离线夹具

仍在 **Linux 宿主**执行，将仓库内的[夹具工具](../../scripts/manual-test-lab.py)复制到客机：

```bash
bash scripts/vm.sh ssh 'mkdir -p /home/tester/clashtui-test/manual; chmod 700 /home/tester/clashtui-test/manual; cat > /home/tester/clashtui-test/manual/lab.py' < scripts/manual-test-lab.py
bash scripts/vm.sh ssh 'python3 /home/tester/clashtui-test/manual/lab.py prepare'
```

预期输出 `Fixtures ready`；准备阶段使用客机 Mihomo 生成真实 MRS 文件。工具只允许测试客机的 `tester` 用户执行，拒绝在宿主、Mac 或 root 下启动。

| 文件 / URL | 用途与预期 |
|---|---|
| `basic.yaml` | 两个 direct 节点、`Manual Select` 手选组、`Manual Auto` 自动组；无公网目标 |
| `providers-yaml.yaml` | HTTP 代理 Provider + YAML domain 规则 Provider |
| `providers-text.yaml` / `providers-mrs.yaml` | 合法 text / MRS domain 规则 Provider |
| `template.yaml` | Provider 分组、生成预览和内联代理测试 |
| `empty-template.yaml` | 不引用 Provider 占位符，专门测试显式空分组 |
| `invalid-core.yaml` | YAML 结构合法、规则非法，用于核心校验及激活拒绝 |
| `escape-parent.yaml` / `escape-absolute.yaml` / `escape-symlink.yaml` | 三种越界缓存路径 |
| `/subscription.yaml` | URL 订阅；节点随 `version` 文件变化；额度为上传 10、下载 20、总量 100 字节 |
| `/proxy.yaml` | 代理 Provider；创建 `fail-proxy` 文件后返回 HTTP 503 |
| `/rules.yaml` / `/rules.text` / `/rules.mrs` | 对应格式的规则集 |
| `/bad.mrs` / `/malformed.yaml` | 损坏的 MRS / YAML |
| `/probe` | 204，用于离线测速与健康检查 |
| `/auth.yaml` | 要求 HTTP Basic：用户名 `manual`，密码 `manual-pass`，均为虚构测试值 |
| `/redirect` / `/unauthorized` / `/fail` / `/slow.yaml` | 302 / 401 / 503 / 延迟 10 秒 |
| `/hello` / `/stream` | 小响应 / 持续传输，用于连接、日志与指标 |

`prepare` 不导入 Profile、不改核心配置、不启用 DNS/TUN。重跑它会更新夹具文件，但不会重置 `version` 或删除 `fail-proxy`；复现前按用例显式设置这两个状态。

夹具工具已在当前 Linux VM 单独验证：5 个正常配置通过核心校验，非法规则被拒绝，真实 MRS 可解码，HTTP 认证/重定向/额度/失败状态及回环 DNS 正常，流量工具产生两条真实客机代理连接。验证记录在 `target/manual-results/fixture-verification-20261002.json`。这只是准备工具的验证，不证明相应业务断言或 Mac 人工体验已经通过。

## 3. 在 Mac 建立连接

### 3.1 SSH 与浏览器隧道：终端 A

在 **Mac** 新开终端，将下列地址替换为实际 Linux SSH 用户及 IP；这里是普通 SSH 登录地址，不是 VM 地址。

```bash
export CT_LINUX='yulinye@实际Linux地址'
ssh "$CT_LINUX"
```

首次连接按实际主机信息核对 SSH 指纹。能登录后 `exit` 返回 Mac，建立转发：

```bash
ssh -N -o ExitOnForwardFailure=yes -o ServerAliveInterval=15 \
  -L 127.0.0.1:39090:127.0.0.1:29090 \
  -L 127.0.0.1:39091:127.0.0.1:29091 \
  "$CT_LINUX"
```

预期：命令持续运行，通常没有输出。**保持终端 A 不关闭**。这依赖第 2 节已经建立的 Linux→客机隧道；单独建立 Mac→Linux 隧道不会启动 VM。

若提示端口占用，在 Mac 检查：

```bash
lsof -nP -iTCP:39090 -sTCP:LISTEN
lsof -nP -iTCP:39091 -sTCP:LISTEN
```

不要结束不属于本轮测试的进程。可将 Mac 本地端口改成其他空闲值，同时替换后文 Mac 页面地址与 MetaCubeXD 端点；Linux 目标 29090/29091 保持不变。

### 3.2 进入客机：终端 B（CLI）、C（TUI）、D（夹具服务）

每个 **Mac** 新终端都执行一次，替换实际 Linux 地址：

```bash
export CT_LINUX='yulinye@实际Linux地址'
ssh -tt "$CT_LINUX" 'cd /home/yulinye/clashtui && bash scripts/vm.sh ssh'
```

预期最后提示符属于 `tester@clashtui-test`。`-tt` 为两层 SSH 的 TUI 操作提供真实终端。每个客机 shell 都初始化：

```bash
hostname
id -un
export CT_CONFIG=/home/tester/clashtui-test/config
export CT_LAB=/home/tester/clashtui-test/manual
ct() { clashtui --config-dir="$CT_CONFIG" "$@"; }
```

必须观察到 `clashtui-test` / `tester`。后文标记 **客机 B/C/D/E** 的命令，都在这些 shell 执行。`ct` 包装函数确保每次操作都使用测试目录。

在 **客机 D** 启动夹具服务，保持运行：

```bash
python3 "$CT_LAB/lab.py" serve
```

预期显示 HTTP `127.0.0.1:18080` 和 DNS 上游 `127.0.0.1:18053`。HTTP 请求会打印路径与状态码，不打印 Authorization。服务绑定客机回环，不向 Mac/LAN 开放。

在 **客机 B** 验证：

```bash
python3 -c 'import urllib.request; print(urllib.request.urlopen("http://127.0.0.1:18080/hello").read().decode())'
ct core status
ct service status
```

预期 `manual-lab-ok`，核心身份为 Mihomo，模式和配置可读，测试服务在运行。

### 3.3 将测试文件下载到 Mac，供浏览器“导入文件”使用

在另一个 **Mac 本地**终端执行，注意不是客机 shell：

```bash
export CT_LINUX='yulinye@实际Linux地址'
export CT_MAC_DIR="$HOME/Desktop/clashtui-manual"
mkdir -p "$CT_MAC_DIR"
scutil --proxy > "$CT_MAC_DIR/mac-proxy-before.txt"
scutil --dns > "$CT_MAC_DIR/mac-dns-before.txt"
route -n get default > "$CT_MAC_DIR/mac-default-route-before.txt"
for fixture in basic.yaml providers-yaml.yaml providers-text.yaml providers-mrs.yaml template.yaml empty-template.yaml invalid-core.yaml escape-parent.yaml escape-absolute.yaml escape-symlink.yaml; do
  ssh "$CT_LINUX" "cd /home/yulinye/clashtui && bash scripts/vm.sh ssh 'cat /home/tester/clashtui-test/manual/$fixture'" > "$CT_MAC_DIR/$fixture"
done
ls -l "$CT_MAC_DIR"
```

预期每个文件非空。在 Mac 用文本编辑器查看 `basic.yaml`，应包含 `Manual Select`；这些 `.yaml` 文件使用 JSON 写法，属于合法 YAML，不需要转换格式。Mac 修改某个导入文件不会自动修改客机，必须通过管理页导入或保存。

### 3.4 管理令牌与核心 secret

在 **客机 B** 读取两种不同凭据：

```bash
cat "$CT_CONFIG/management-token"
printf '\n'
python3 - <<'PY'
from pathlib import Path
import yaml
p = Path('/home/tester/clashtui-test/config/mihomo/core_override_config.yaml')
print(yaml.safe_load(p.read_text())['secret'])
PY
```

第一种填管理页，第二种填 MetaCubeXD。仅复制到本轮页面，不写入地址栏、测试报告或截图。此目录只包含独立测试凭据，不应替换为宿主真实凭据。

## 4. 记录方式与通过标准

每项用例使用 `PASS / FAIL / BLOCKED / SKIP`：

- PASS：完成操作，并用另一个端或文件/核心 API 核对最终结果。
- FAIL：行为不符；记录触发动作、预期、实际结果、错误正文与时间。
- BLOCKED：隧道、权限、夹具或终端依赖不足，还没有验证业务行为。
- SKIP：经确认当前核心/平台不支持，写明版本及缺失能力；不能记作 PASS。

建议复制[简化记录模板](manual_test_record_zh.md)。H01–H06 单独记录；原 M 用例仅在专项复现时追加。自动化结果按新方案单独引用，不把未覆盖子项填作通过。

页面通常约 2 秒刷新；跨端观察等待 2～5 秒。TUI 文件列表若未刷新，先离开 Files 再返回；节点/资源页可按 `r`。测速、服务启动与下载按其超时等待。持续不刷新、需重启才能看到、或鼠标点击频繁被刷新打断，应记录为问题，而不是反复点击后记为正常。

保存成功只证明文件提交；激活成功还需要核心运行值一致。点击后的提示、CLI 退出码、文件内容、当前 Profile、实际代理连接分别记录，避免只看到“操作完成”就判通过。

## 5. 基础界面、认证与文件工作流

### M01：版本、认证和两个 Web 入口

1. **客机 B**：运行 `ct --version`、`/opt/clashtui/bin/mihomo -v`、`ct manage state`，记录版本和当前 Profile。
2. **Mac 浏览器**：打开 `http://127.0.0.1:39091/`，先填写虚构错误令牌 `wrong-token`，点击“连接 / 刷新”。
3. 观察拒绝认证、主操作区未开放；换管理令牌后，应出现订阅、模板、运行设置和服务区域，当前 Profile 为 `baseline`。
4. 手动打开 **`http://127.0.0.1:39090/ui/`**。MetaCubeXD 中添加端点 **`http://127.0.0.1:39090`**，先用错误 secret 验证拒绝，再用正确核心 secret 连接。
5. 观察面板可以读取 `Test Select`、规则、连接及核心状态；不是只有静态页面加载成功。若启用错误端点、不可达端点，也应显示明确错误。
6. 管理页“打开 MetaCubeXD”链接来自客机配置，可能指向 Mac 上不存在的 `127.0.0.1:9090/ui/`。本拓扑使用上面的 Mac 转发地址手动打开，不为此修改客机控制地址。

预期：两种凭据不能互换；令牌不出现在 URL。管理页标题中的“目标版本 v1.273.1”不等于实际资源验证；基线面板由锁定归档复制，管理页可能显示“实际版本未知”，应结合 VM 的归档摘要与面板资源记录，而不是把此状态误认为业务失败。

截图：认证拒绝、管理页状态、MetaCubeXD 节点页各一张，隐藏凭据。

### M02：远程 TUI 导航、帮助与窗口尺寸

**客机 C**运行 `ct`。Mac 终端先设置约 140 列 × 40 行，再测试约 100×30 和 80×24；最小尺寸显示截断也要观察是否可恢复。

| 键 | 默认页面 / 动作 |
|---|---|
| `1`～`9` | Status、Files、Proxies、Connections、Logs、Settings、Service、Rules、Providers |
| `?` | 当前页帮助；核对实际快捷键，以帮助为准 |
| `j/k` 或上下键 | 移动 |
| `Esc` | 取消输入/确认、退出详情或清除筛选，依当前页 |
| `Ctrl-C` | 正常退出 TUI；退出后 shell 仍可正常输入 |

1. 依次切换九页，打开帮助后关闭；观察是否有残留、重叠或界面冻结。
2. 缩小、放大终端，重复输入中文、空格及搜索；确认选中项和边框没有错位。
3. 按 `Ctrl-C`，确认退出码 0、光标恢复、shell 回显正常；再次运行 `ct`。
4. 在没有 GUI 的客机，TUI 打开浏览器、系统剪贴板和默认 `xdg-open` 编辑器可能不可用。Mac Terminal 只是远程显示，不会把这些动作自动转交 Mac 的 `open`/`pbcopy`。记录为环境限制，浏览器和剪贴板测试按本文 Mac 页面操作；不能据此认定已验收 Mac 原生入口。

纯导航前后可在 **客机 B**执行 `sha256sum "$CT_CONFIG/clashtui.db"`，预期相同；期间不要同时改名、添加、切换 Profile 或切换保存选项。

### M03：Web 导入，CLI/TUI 读取与筛选

1. 管理页“配置名称”填写 `manual-basic`，选择 Mac 的 `basic.yaml`，点击“导入文件”。
2. 观察新增一行 `manual-basic`，类型为文件配置；当前 Profile 仍为 `baseline`，导入没有隐式激活。
3. **客机 B**运行：

```bash
ct profile list
ct manage read --name manual-basic
```

4. **TUI Files**：找到 `manual-basic`，按 `p` 预览；观察 `Manual A v1`、`Manual B v1`、`Manual Select`。`Esc` 关闭。
5. Web 筛选名称 `manual-`，TUI 按 `/` 输入 `manual-basic`。应只操作可见选中行；清除筛选后完整列表恢复。
6. 管理页再次用同名导入，观察拒绝覆盖既有配置；原文件和当前 Profile 不变。

### M04：编辑、导出、改名与删除保护

1. Web `manual-basic` 行点击“编辑文件”，把一个节点名及其组引用一起从 `Manual B v1` 改为 `Manual B edited`，点击“保存”。
2. CLI `ct manage read --name manual-basic`、TUI `p` 都应看到新名字。核心当前仍是 baseline；保存不应隐式激活。
3. 点击编辑区“导出”，Mac 下载文件应等于编辑内容，文件不应混入管理令牌或核心 secret。
4. 点击“编辑名称/URL”，将配置改名 `manual-renamed`。CLI/TUI/管理页都应移除旧名、出现新名；文件名随之改变，配置内容保留。
5. 再改回 `manual-basic`。点“删除”后在浏览器确认框**取消**；行和文件应仍存在。
6. 后续 M07 激活 `manual-basic` 后尝试删除：应拒绝删除当前 Profile；`baseline` 激活后才允许删除非当前测试文件。此处保留 `manual-basic` 供后续测试，不立即删除。

CLI 写入另一个修改版本时：

```bash
ct manage read --name manual-basic > "$CT_LAB/document.json"
python3 - <<'PY'
import json
from pathlib import Path
lab = Path('/home/tester/clashtui-test/manual')
v = json.loads((lab/'document.json').read_text())
p = json.loads(v['content'])
p['log-level'] = 'debug'
(lab/'edited.yaml').write_text(json.dumps(p))
PY
CT_REV=$(python3 -c 'import json,os; print(json.load(open(os.environ["CT_LAB"]+"/document.json"))["revision"])')
ct manage save --name manual-basic --input "$CT_LAB/edited.yaml" --revision "$CT_REV"
```

预期 Web/TUI 回读含 `log-level: debug` 或对应 JSON 字段。这里的 Python 修改方法依赖本用例尚保持 JSON 写法；若手工改成普通 YAML，使用 `yaml.safe_load`/`yaml.safe_dump`。

### M05：双编辑器冲突与只读命令不破坏修订

**文档冲突**：

1. 用同一浏览器两个标签页 A/B 连接管理页，同时编辑 `manual-basic`。
2. A 保存 `log-level=info`；B 未重新打开文档，保存 `log-level=warning`。
3. B 应报 `Revision conflict`，A 的内容保持；B 的未保存文本应可复制。仅点击“连接 / 刷新”不会重读已打开文档，必须关闭并重新点击“编辑文件”再保存。
4. 重新加载后 B 保存成功，CLI 和 TUI 回读应与 B 一致。

**只读修订回归**：管理页定时刷新可能掩盖缺陷，因此用 Mac Chrome 开发者工具（⌥⌘J）固定一次状态。先在控制台执行：

```javascript
const ctHeaders = {Authorization: 'Bearer ' + document.querySelector('#token').value};
const ctBefore = await (await fetch('/api/state', {headers: ctHeaders})).json();
console.log({core: ctBefore.core, revision: ctBefore.revision});
```

接着 **客机 B**执行，不进行任何业务修改：

```bash
sha256sum "$CT_CONFIG/clashtui.db"
for attempt in {1..10}; do
  ct core status >/dev/null
  ct manage state >/dev/null
  ct manage read --name manual-basic >/dev/null
done
sha256sum "$CT_CONFIG/clashtui.db"
```

回到同一个浏览器控制台，使用刚才的原始修订：

```javascript
const ctReply = await fetch('/api/action', {
  method: 'POST', headers: {...ctHeaders, 'Content-Type': 'application/json'},
  body: JSON.stringify({action: 'check', name: 'manual-basic', core: ctBefore.core, revision: ctBefore.revision})
});
console.log(ctReply.status, await ctReply.json());
```

预期两个摘要相同，HTTP 200 并返回 job，任务最终完成；不是错误修订冲突。可在 Network 中查看 `/api/job`，或稍后控制台读取 `await (await fetch('/api/job',{headers:ctHeaders})).json()`。浏览器声明同名常量不能重复执行，重测时刷新页面或换变量名。不要在控制台直接打印 `ctHeaders`。

### M06：非法保存、核心诊断与拒绝激活

1. 编辑 `manual-basic`，输入 `[unterminated` 并保存。应立即报告 YAML 错误，原文件内容与修订不变；重新加载仍能看到之前有效内容。
2. 导入 Mac 的 `invalid-core.yaml`，命名 `manual-invalid`。该文件 YAML 结构合法，允许保存，但它的规则语义非法。
3. Web 分别点“校验”“测试”；TUI Files 选中此 Profile，分别按 `c`、`t`；CLI：

```bash
ct manage check --name manual-invalid
echo "check exit=$?"
ct manage test --name manual-invalid
echo "test exit=$?"
```

4. 预期三端都有核心诊断；JSON `valid=false`、`activated=false`、非零 `exit_code`，CLI 退出非零。Check/Test 不修改当前核心。
5. 激活前记录 `ct manage state` 和 `sha256sum "$CT_CONFIG/mihomo/config.yaml"`。Web 点“激活”并确认，CLI 可独立重测：

```bash
ct manage activate --name manual-invalid --yes
echo "activate exit=$?"
```

6. 预期拒绝、退出非零；当前 Profile 和有效运行文件摘要不变，`ct core status` 仍可读。

恢复：重新打开 `manual-basic` 原有效文件，不要把非法文本当作后续夹具。

### M07：激活、节点选择与三端同步

1. Web 激活 `manual-basic` 并确认。CLI `ct manage state` 显示当前为 `manual-basic`；TUI Files 当前标记对应此行。
2. Mac MetaCubeXD Proxies 页应出现 `Manual Select`、两个测试节点和 `Manual Auto`，TUI `3` 页也应一致。
3. MetaCubeXD 展开 `Manual Select`，选择 `Manual A v1`；CLI 核对：

```bash
ct core proxies
```

4. 在返回的 `proxies["Manual Select"].now` 中观察该名字；TUI 按 `r` 后选择标记一致。
5. TUI 选中 `Manual Select` 的另一个节点并 Enter；MetaCubeXD 刷新/轮询后应显示新选择，CLI 再核对。
6. CLI 执行 `ct core select 'Manual Select' DIRECT`，两端都应显示 DIRECT。
7. 若自动组支持固定选择：在 `Manual Auto` 固定某节点，再在 TUI 按 `u` 或执行 `ct core unfix 'Manual Auto'`，观察恢复自动选择。当前版本若明确返回不支持，记录能力跳过及错误，不臆测已经恢复。
8. 选择 REJECT 仅用于短暂观察 UI，同步检查后恢复 DIRECT；后续流量测试需要可通的选择。

### M08：URL 订阅、额度、更新与元数据修改

1. Web 名称 `manual-sub`，URL **`http://127.0.0.1:18080/subscription.yaml`**，点“添加订阅”。这里是客机地址，不是 Mac 转发地址。
2. 预期新增 URL Profile；CLI `ct manage profile_url --name manual-sub` 返回原 URL；Web“复制 URL”在 Mac 剪贴板应得到同样文本。浏览器不允许剪贴板时，应显示可手动复制的结果。
3. Web“流量额度”、TUI Files `n`、CLI `ct manage traffic --name manual-sub` 核对上传 10、下载 20、总量 100 字节，已用 30/100=30%；过期时间来自夹具的 `2000000000`。显示单位和时区可不同，数值含义应一致。
4. **客机 B**执行 `printf '2\n' > "$CT_LAB/version"`。先观察已保存 Profile 仍是旧节点，再点 Web“更新”。
5. CLI 文件回读应变为 `Manual A v2` / `Manual B v2`；TUI 预览一致。更新非当前 Profile 不应改变正在运行的 manual-basic。
6. 激活 `manual-sub` 后，将 version 改为 3，再用 CLI `ct manage update --name manual-sub`；预期当前 Profile 成功更新后也重新激活，MetaCubeXD 与 TUI 节点页显示 v3。
7. “编辑名称/URL”把 URL 改成 `/auth.yaml`，先不用用户名：元数据保存可以成功，但更新应返回 401。再改为 `http://manual:manual-pass@127.0.0.1:18080/auth.yaml`，更新应成功。测试密码是虚构值；不要将实际订阅密钥录入截图。
8. 单独创建 `/redirect`、`/slow.yaml`、`/malformed.yaml`、`/fail` 的订阅，分别观察重定向、超时、解析失败、HTTP 503。失败的新订阅不得留下已注册但无有效文件的条目；超时不应无限忙碌。当前全局 timeout 是 5 秒，slow 响应为 10 秒，记录实际耗时。
9. 测试结束把 version 恢复为 1；后续模板/Provider 示例使用 v1。

### M09：内联代理、代理更新选项及批量失败

1. 在 `manual-sub` 行分别点击“内联 Provider”和“代理更新”，观察 ✓；TUI Files 对应 `P`、`O`，CLI `ct manage state` 的 `no_pp`、`update_with_proxy` 同步变化。
2. “代理更新”打开后，更新订阅应经过**客机** 7890；夹具可达且选择可通时成功。明确覆盖一次保存选项：

```bash
ct manage update --name manual-sub --with-proxy false
ct manage state
```

预期本次不走代理，但保存的选项仍保持原值；测试后关闭 ✓，避免后续停止核心时订阅也被代理选项影响。
3. 批量测试在后面的 M12 制造失败后执行。Web“更新全部配置”结果应逐项保留成功/失败；CLI `ct manage update_all --yes` 部分失败时退出非零；TUI 的批量更新入口按帮助确认，失败资源不能被成功行掩盖。
4. `no_pp` 内联的是代理订阅；规则 Provider 与 RULE-SET 引用保留，具体格式和缓存检查见 M13。

## 6. 模板、Provider 与五项缺陷回归

### M10：新建模板、生成预览、生成与引用保护

1. 管理页“新建模板”，输入 `manual-template.yaml`。编辑区粘贴 Mac 的 `template.yaml` 全部内容，保存。
2. CLI `ct manage read --kind template --name manual-template.yaml` 回读；TUI Files 用左右键切到模板侧，找到它，按 `p` 看源内容。
3. Web 点“编辑 Provider”，JSON 应为 `manual → Manual HTTP → http://127.0.0.1:18080/proxy.yaml`。TUI 模板侧 `E` 查看/编辑对应元数据。先取消一次，确认没有修改。
4. Web“生成预览”、TUI 模板 `P`、CLI `ct manage preview_template --name manual-template.yaml` 都应显示展开后的 Provider 名与组引用，而不是未解析的 `${PPG.manual}`。预览不应注册新 Profile。
5. Web“生成的配置名称”填 `manual-generated`，点击“生成配置”。CLI `ct manage state`、TUI Profile 侧应出现新的 Template Profile。
6. **先更新，再激活**：生成不等于下载 Provider 缓存。运行：

```bash
ct manage update --name manual-generated
ct manage check --name manual-generated
ct manage activate --name manual-generated --yes
```

7. 观察核心出现 `Manual Template Select` 与 `Manual Provider v1`，MetaCubeXD/TUI/CLI 回读一致。
8. 再次生成同名：取消确认应保留文件；确认替换后仍应保留 Profile 的 `no_pp`、代理更新选项。CLI 重生成同名需 `--yes`。
9. Web 删除仍被引用的 `manual-template.yaml` 应拒绝。删除当前 `manual-generated` 也应拒绝。先激活 `manual-basic`，再删除 generated Profile，模板才可删除；本轮后续 M12 还需要它，先不要删除。

检查边界：模板名称中尝试 `../bad.yaml` 应拒绝；不能覆盖同名普通 File/URL Profile。失败时已有文件与注册信息保持。

### M11：明确清空 Provider 分组，不恢复旧文件

本用例使用专门的 `empty-template.yaml`。普通 template 中的 `${PPG.manual}` 在分组清空后可能缺少引用，这是模板本身的语义错误；不要用这种错误判断“空分组是否被正确保存”。

1. 在客机保存已有旧文件（若存在），再写入测试旧分组：

```bash
python3 - <<'PY'
from pathlib import Path
import json
root = Path('/home/tester/clashtui-test/config/mihomo')
lab = Path('/home/tester/clashtui-test/manual')
p = root/'template_proxy_providers.yaml'
(lab/'legacy-was-present').write_text('yes' if p.exists() else 'no')
if p.exists():
    (lab/'legacy-original.yaml').write_bytes(p.read_bytes())
p.write_text(json.dumps({'legacy': {'old': 'http://127.0.0.1:18080/proxy.yaml'}}))
PY
```

2. Web 新建 `manual-empty.yaml`，粘贴 Mac 的 `empty-template.yaml` 并保存。点击“编辑 Provider”，明确输入 `{}`、保存，再打开应仍为 `{}`。
3. CLI `ct manage template_providers --name manual-empty.yaml` 的 `groups` 应为空；TUI `E` 查看同样为空。
4. 生成预览、生成 `manual-empty-generated`，查看生成文件。`clashtui.proxy_provider_groups` 应是 `{}`，不能出现 `legacy.old`。
5. 退出、重进 TUI；刷新浏览器；CLI 另起进程读取，都不能把旧分组恢复回来。
6. 恢复旧文件测试现场：

```bash
python3 - <<'PY'
from pathlib import Path
root = Path('/home/tester/clashtui-test/config/mihomo')
lab = Path('/home/tester/clashtui-test/manual')
p = root/'template_proxy_providers.yaml'
if (lab/'legacy-was-present').read_text() == 'yes':
    p.write_bytes((lab/'legacy-original.yaml').read_bytes())
else:
    p.unlink(missing_ok=True)
PY
```

### M12：下载失败时缓存保留，但必须报告失败

1. 确认夹具服务仍在；**客机 B**执行：

```bash
rm -f "$CT_LAB/fail-proxy"
printf '1\n' > "$CT_LAB/version"
ct manage update --name manual-generated
```

2. 保存代理缓存摘要，`find` 输出只作观察，不删除缓存：

```bash
find "$CT_CONFIG/mihomo/proxies" -type f -exec sha256sum {} \; > "$CT_LAB/cache-before.txt"
touch "$CT_LAB/fail-proxy"
```

3. Web `manual-generated` 行点“更新”。观察部分失败提示；结果资源 `ok=false`、错误含 HTTP 503；不能只有“操作完成”。
4. TUI Profile 侧选中它，按 `u`。对话框应含 FAILED 和下载错误，标题提示部分失败。
5. CLI 单次及批量检查：

```bash
ct manage update --name manual-generated
echo "single exit=$?"
ct profile update --name manual-generated --with-proxy=false
echo "legacy single exit=$?"
ct manage update_all --yes
echo "batch exit=$?"
find "$CT_CONFIG/mihomo/proxies" -type f -exec sha256sum {} \; > "$CT_LAB/cache-after.txt"
diff -u "$CT_LAB/cache-before.txt" "$CT_LAB/cache-after.txt"
```

预期单次、批量都非零；失败下载没有改旧缓存。若批量另外成功更新了其他 Provider，整体目录差异可能合法，应逐一核对 generated 的缓存文件。Web 批量结果须保留其他成功项，不能因一个 503 丢失整个结果。

TUI 默认 Profile `U` 绑定 `UpdateAll`；执行后核对混合批次的逐项失败和成功回读，不能只看弹窗出现。

恢复：

```bash
rm -f "$CT_LAB/fail-proxy"
ct manage update --name manual-generated
```

预期再次更新成功；保存的代理更新选项保持，不应因失败被偷偷切换。

### M13：YAML/text/MRS、损坏文件及内联代理语义

1. Web 分别导入 `providers-yaml.yaml`、`providers-text.yaml`、`providers-mrs.yaml`，命名 `manual-providers`、`manual-text`、`manual-mrs`。
2. 对三个 Profile 分别“更新”“校验”“激活”。CLI 也可依次执行，每一步检查退出码：

```bash
for name in manual-providers manual-text manual-mrs; do
  ct manage update --name "$name"
  ct manage check --name "$name"
  ct manage activate --name "$name" --yes
done
```

3. 每次激活后在 TUI `8`、`9` 和 MetaCubeXD Rules/Providers 对照：`Manual Rules` 是 domain 规则 Provider，核对核心提供的规则数、行为及规则引用；format 从配置文件核对，API 未提供时不要求页面显示。text/MRS 不应报 YAML Mapping 错误。
4. 对 `manual-mrs` 打开“内联 Provider”开关，再激活，读取实际配置：

```bash
python3 - <<'PY'
from pathlib import Path
import yaml
p = Path('/home/tester/clashtui-test/config/mihomo/config.yaml')
v = yaml.safe_load(p.read_text())
print(v.get('rule-providers'))
print(v.get('rules'))
PY
```

预期规则 Provider 保留 `format: mrs`，`RULE-SET,Manual Rules,DIRECT` 仍在原位置；不能把原始 domain 列表直接追加成无策略的普通规则。对 text 重复。

5. 在 `manual-providers` 开内联代理并激活，实际配置中代理 Provider 被展开成普通节点，组能选择 `Manual Provider v1`；规则 Provider 继续保留。
6. 测试损坏 MRS：先保存 `sha256sum "$CT_CONFIG/mihomo/rules/manual.mrs"`。编辑 `manual-mrs` 的订阅文件，只把规则 Provider URL 的 `/rules.mrs` 改为 `/bad.mrs`，缓存 path 仍为 `rules/manual.mrs`。
7. 保存可以成功，但“更新”必须失败；CLI 退出非零，错误表明 MRS 数据无效；原缓存摘要不变。`mihomo -t` 本身不会证明 MRS 缓存完整，因此这里必须检查下载结果和实际缓存。
8. 把 URL 改回 `/rules.mrs`，更新恢复成功。恢复 `manual-providers` 的内联开关为关闭，再更新并激活，供 M14 使用。

### M14：Provider API、健康检查、测速、搜索和导出

前提：`manual-providers` 当前激活，`fail-proxy` 不存在。

1. TUI `9` 页显示代理 Provider，`t` 切换规则 Provider；用 `/` 搜索 `Manual`，Enter 查看详情，`Esc` 关闭；MetaCubeXD 对应页面核对。
2. CLI：

```bash
ct core resources proxy-providers list
ct core resources rule-providers list
ct core resources rules list
```

3. version 改为 2，分别通过 MetaCubeXD、TUI `u`、CLI 更新 `Manual HTTP`；观察运行核心的 Provider 节点变成 v2。Provider API 更新与“本地 Profile 文件更新”是两个入口，分别记录，不能只测其中一个。
4. TUI `h` 或 MetaCubeXD 健康检查；CLI：

```bash
ct core resources proxy-providers health --name 'Manual HTTP' --yes
ct core delay 'Manual Provider v2' --provider 'Manual HTTP' --url http://127.0.0.1:18080/probe --timeout 3000
ct core delay 'Manual Select' --group --url 'http://127.0.0.1:18080/probe?x=hello%20world&z=%252F' --timeout 3000
```

预期本地测试节点获得成功延迟；REJECT 出现失败是预期；URL 编码在夹具请求日志中保持，不应被重复解码改变。TUI/MetaCubeXD 的测速 URL 若仍是公网默认值，离线失败不代表算法失效：先查看能否配置成本地 `/probe`，不能配置时记录离线限制，CLI 本地测速另记 PASS。

5. TUI `d` 输入 Provider 中的**完整节点名**；不存在的节点应拒绝而不是误测别的节点。
6. TUI `U`、MetaCubeXD 批量更新、CLI `update-all --yes` 对照；健康检查仅适用于代理 Provider，规则 Provider 没有此动作时应禁用或明确不支持。
7. TUI 筛选后按 `e`，导出路径填 **客机** `$CT_LAB` 的绝对路径，例如 `/home/tester/clashtui-test/manual/provider-visible.json`，不要输入 Mac Desktop 路径。读取文件应只包含可见结果；已存在的目标不能静默覆盖。
8. 规则顺序按 API 原顺序保留。Mihomo 1.19.24 不暴露 disabled 状态，规则开关记 `SKIP: core capability`；TUI/MetaCubeXD/CLI 不应伪报切换成功。更换支持该接口的核心后另测，不能把所有规则当作可停用。

### M15：路径越界、绝对路径及符号链接防护

**客机 B**准备只能属于本轮测试的哨兵和链接：

```bash
printf 'preserve-manual-sentinel\n' > "$CT_CONFIG/manual-sentinel.yaml"
ln -s "$CT_CONFIG" "$CT_CONFIG/mihomo/manual-link"
sha256sum "$CT_CONFIG/manual-sentinel.yaml"
```

如果同名链接已存在，先确认是否为本轮链接；不要覆盖未知文件。

1. Web 导入三种 escape 文件，分别命名 `manual-escape-parent`、`manual-escape-absolute`、`manual-escape-symlink`。
2. 三者均点“更新”；预期部分失败并给出越界/符号链接错误，不执行目录外写入。
3. 尝试激活三者，均应拒绝；不能因为外部文件已经存在就绕过路径校验。
4. 每一步后核对哨兵摘要和文本仍相同，当前运行 Profile/配置文件没有被失败激活替换。
5. 正常 `manual-providers` 更新仍可写 `mihomo/proxies/`、`mihomo/rules/`，不能把所有相对路径都误拒绝。

恢复：仅删除本轮链接 `rm "$CT_CONFIG/mihomo/manual-link"`。默认不接受额外 SAFE_PATHS；本用例不测试恶意本地进程并发替换目录的竞态。

## 7. 真实流量、指标、日志和运行设置

### M16：持续流量与指标三端观察

1. 激活 `manual-basic`，设置 Rule 模式和 DIRECT 选择：

```bash
ct manage activate --name manual-basic --yes
ct mode set rule
ct core select 'Manual Select' DIRECT
```

2. 再开一个 Mac→客机终端 **E**，初始化第 3.2 节变量/函数，运行：

```bash
python3 "$CT_LAB/lab.py" traffic --seconds 300
```

工具通过**客机代理 7890**发起两个持续流，目标分别为 `localhost:18080` 和 `127.0.0.1:18080`；不配置任何机器的系统代理。结束或 Ctrl-C 后关闭连接。

3. Mac MetaCubeXD Overview 图表、TUI Status `1` 应出现非零下载速率/流量增长；CLI：

```bash
ct core metrics --samples 10 --interval-ms 1000 > "$CT_LAB/metrics.json"
cat "$CT_LAB/metrics.json"
```

4. 观察多次采样中的字节数增长、连接数与实际连接对应；瞬时数值不必完全相等，各端采样时刻不同。
5. 停止流量后，速率应降到接近零，而会话累计量可保留。重启核心后重新发流，采样基线重置，不能出现负速率或把旧会话累计量当作本轮瞬时流量。

### M17：连接详情、过滤、关闭固定集合及导出

1. 重新启动 300 秒流量。TUI `4`、MetaCubeXD Connections、CLI `ct core connections` 找到两个流。
2. 对照 ID、host/destination、端口、匹配规则、代理链、上传/下载。Linux direct 连接不一定有完整进程信息，缺字段应显示未知，不能捏造值。
3. 用 `localhost` 筛选，另一条 `127.0.0.1` 的流应被隐藏。先记录两个实际 ID；其他健康检查短连接可能出现，不用“全局连接数必须等于 2”作为唯一断言。
4. TUI 筛选后 Enter 看详情，`e` 导出 `/home/tester/clashtui-test/manual/connections-visible.json`。预期只有筛选集合，字段未被无意删减。
5. TUI `dd` 关闭选中连接；MetaCubeXD 回读该 ID 消失，另一条原 ID 仍在。重新发流，再用 CLI：

```bash
ct core connections --filter localhost
ct core connections --filter localhost --close --yes
```

6. 预期关闭操作只针对捕获的匹配集合，未匹配 ID 保留。MetaCubeXD 中也按搜索/选中集合关闭一次，三端独立记录。
7. 清除筛选后剩余连接恢复显示；暂停/恢复列表（TUI `p`）不应断开连接。CLI `--id 实际ID` 详情与页面一致。

### M18：日志级别、筛选、暂停、清空、导出和重连

1. Web“编辑临时设置”填 `{"log-level":"debug"}`。通过持续流、一次成功订阅和一次失败订阅产生事件。
2. TUI `5` 和 MetaCubeXD Logs 应收到新日志。按当前帮助切换级别、`/` 搜索测试节点或 localhost；无匹配行不是失败，但应确认筛选清除后恢复。
3. TUI `p` 暂停，再产生事件；显示应暂停，恢复后继续接收。`c` 清空显示不应重启核心或删配置。
4. TUI `e` 导出唯一文件路径；MetaCubeXD 使用其导出入口（若当前页面提供）；CLI 有限采集：

```bash
ct core logs --level debug --seconds 10 --limit 100 --output "$CT_LAB/logs-capture.json"
echo "logs exit=$?"
```

预期最长受时间/条数限制，不无限等待；再写同一路径应拒绝覆盖。日志筛选按大小写/实际文本核对，截图及导出不包含测试令牌或核心 secret。

5. 核心停止/重启后观察断线提示和重连：不应永久显示旧数据却伪装在线；重连后只建立必要订阅，不能成倍重复同一事件。

### M19：模式、连接关闭策略与临时/持久设置

**模式互相可见**：MetaCubeXD 改为 Direct，TUI Settings `6` 和 CLI `ct core status` 应是 direct；TUI Enter 打开 Mode 选择，选 Global，再由 Web/CLI 回读；最后 CLI `ct mode set rule`，两端回读 rule。未被核心 mode-list 声明的自定义模式不作为本版本必测成功项。

**关闭连接策略**：

1. 发起持续流，记录原 ID。不启用关闭策略时改变模式，原连接通常继续存在；按核心行为记录，不把新请求与旧 ID 混为一谈。
2. 在 TUI Settings 按 `c` 打开“模式变化后关闭连接”，再改模式。原 ID 应消失；该策略是本 TUI 会话的选项，不要求 MetaCubeXD 展示相同开关。
3. 重新发流，用 `ct mode --close-connections set rule` 验证 CLI。操作成功后旧连接集合被关闭；模式切换失败不能误关闭连接。
4. 关闭 TUI 本地 `c` 选项，恢复 Rule / DIRECT。

**运行设置与持久化**：

1. Web“编辑临时设置”输入 `{"mode":"direct","log-level":"debug"}`；TUI/CLI 应自动回读。磁盘覆盖配置尚未因临时修改自动保存。
2. Web“保存运行设置为覆盖配置”，先取消一次；覆盖文件不变。确认保存后，通过 CLI `ct manage read --kind override` 或 Web“编辑覆盖配置”核对持久值。
3. 激活 baseline 或另一个 Profile，持久 direct/debug 应仍生效；服务重启后回读。
4. 编辑覆盖配置改成 `mode: rule`、`log-level: info`（JSON 写法也可），只保存不激活。核心应仍为先前运行值；激活后才变化。
5. TUI `e` 输入临时 JSON、`p` 保存覆盖，独立再测一次并对照 Web/CLI。
6. 非法字段、无法写的字段或不存在的字段应拒绝/明确报告，不能误写控制端点或 secret。不要为测试而直接替换整个覆盖文件，里面包含当前控制端点和密钥。

恢复：Rule、info，保存并激活；维持 mixed-port 7890，DNS/TUN 关闭，控制地址/secret 不变。

## 8. 服务、任务恢复与故障恢复

### M20：三个端的启动、停止和重启

1. 先停止流量，把夹具失败标记移除。分别使用 Web 服务按钮、TUI `7` 中的服务动作、CLI `ct service stop/start/restart` 完成一轮；每次只操作一次，等待结果后再进行下一步。
2. 停止后应观察：客机核心服务 inactive、核心 API 不可达、MetaCubeXD/TUI 提示断线；**管理页服务仍运行**，文件读取和启动入口仍可用。
3. 启动后必须真正能读取核心身份与配置，不能只看到 systemctl 请求已发出就报就绪。`ct core status`、节点列表和 Web/TUI 的连接状态应恢复。
4. 重启后当前有效文件仍存在、控制 secret 不变，日志/指标恢复采样；若节点选择由核心自行持久化，按实际回读记录，不假定每个临时选择都恢复。
5. 单独核对客机：

```bash
systemctl is-active clashtui-test-mihomo
systemctl is-active clashtui-test-web
ct core status
```

6. 服务无权限、就绪超时或身份不符应显示失败，不自动重复操作。当前 tester 在测试客机有非交互 sudo 权限；如果 Web 持续报 sudo 权限问题，先记录环境 BLOCKED，再核对客机服务配置，不修改宿主 sudo。

停止核心时正常的 API 错误与恢复后持续无法连接不同。恢复失败时先导出客机 journal，再按第 11 节恢复基线。

### M21：真实端口占用引发激活失败并回滚

前提：夹具 HTTP 正占用客机 18080，核心 mixed-port 原为 7890，当前有效 Profile 可用。

1. **客机 B**保存前状态：

```bash
ct manage state > "$CT_LAB/rollback-state-before.json"
sha256sum "$CT_CONFIG/mihomo/config.yaml" > "$CT_LAB/rollback-active-before.txt"
```

2. Web“编辑覆盖配置”，**只将** `mixed-port` 改为 `18080`，其余字段特别是控制地址/secret 保留，保存。不应立即改变当前核心监听。
3. 激活 `manual-basic`。核心尝试绑定已占用端口，应失败并恢复旧有效文件/核心。
4. 观察错误正文、当前 Profile、有效文件摘要，`ct core status` 中 mixed-port 应恢复 7890；再次发流还能经 7890 到达夹具。
5. 对照原有效文件摘要：

```bash
sha256sum "$CT_CONFIG/mihomo/config.yaml"
ct core status
```

6. **覆盖文件本身是前一步主动保存的，不属于此次激活回滚范围。** 必须回到 Web 把 mixed-port 改回 7890 并保存，再激活有效 Profile，确认恢复后不再失败。

如果回滚也失败，不反复点激活；先记为 FAIL，保留日志，必要时从 Linux 宿主停止并 reset 客机。此用例不停止宿主任何监听。

### M22：管理服务重启后的任务结果与中断提示

**已完成任务恢复**：

1. Web 对有效 Profile 执行“校验”，等任务结束。在 **客机 B**用下面的只读脚本保存最后任务，不手工传输令牌：

```bash
python3 - <<'PY'
from pathlib import Path
import urllib.request
root = Path('/home/tester/clashtui-test/config')
lab = Path('/home/tester/clashtui-test/manual')
request = urllib.request.Request('http://127.0.0.1:9091/api/job', headers={
    'Authorization': 'Bearer ' + (root/'management-token').read_text().strip()})
(lab/'job-before.json').write_bytes(urllib.request.urlopen(request).read())
PY
sudo systemctl restart clashtui-test-web
```

2. 浏览器重新连接，重复脚本把文件名改为 `job-after.json`。两份 JSON 的 job ID、pending=false 和 result 应一致。页面未自动显示历史结果时以 `/api/job` 为恢复证据，不把普通状态刷新等同于历史结果恢复。
3. 重启后新的正常任务仍能执行，旧任务不应被自动重放。

**执行中任务中断**：

1. 新建一个专用 URL Profile `manual-interrupted`，最初 URL 用正常 `/subscription.yaml`，确保已有有效文件。
2. 将该 Profile URL 改为 `/slow.yaml`。Web 点击更新后，立即在客机 B 执行 `sudo systemctl restart clashtui-test-web`；记录操作时间。
3. 重连后读取 `/api/job`：应提示任务被重启中断、需要刷新核实，不能报成功或自动重新下载。
4. 文件提交可能处于操作的不同阶段，检查实际 Profile 内容、当前核心与有效文件后决定恢复；不能只凭中断提示认定操作“完全没执行”。
5. 若动作已经先超时，则本轮只证明超时处理，需要再做一次并缩短重启时间。恢复 Profile URL 为正常值；结束前确认没有仍忙碌的任务。

### M23：无 GUI 的外部编辑器入口（可选）

功能性已由 CT07/CT10 驱动真实 vi 与失败编辑器断言。H05 只补充实际远程终端的显示和输入体验。

1. 客机执行 `command -v vi`；缺少依赖记 BLOCKED。退出已有 TUI，再配置客机编辑器：

```bash
python3 - <<'PYCONF'
from pathlib import Path
import json, yaml
p = Path('/home/tester/clashtui-test/config/config.yaml')
v = yaml.safe_load(p.read_text())
v.setdefault('extra', {})['edit_cmd'] = 'vi %s'
p.write_text(json.dumps(v))
PYCONF
```

2. 在同一 SSH -tt 终端重启 TUI。Files Profile `E` 编辑可丢弃文件，模板侧 `e` 编辑模板：TUI 应挂起，vi 接管终端。只增添注释，`:wq` 退出；TUI 恢复后预览，Web/CLI 回读。
3. 取消编辑、名字含中文/空格的文件、未知/已删除文件的错误分别记录。编辑不能误操作另一个筛选行。
4. 外部编辑器不参与 Web/CLI 文档修订协议；并发编辑时使用 Web/CLI 保存入口。

恢复：退出编辑器/TUI；本轮结束时 reset 恢复客机客户端配置。远程 TUI 剪贴板不代表 macOS 原生 `pbcopy/open` 已验收。

## 9. DNS / TUN 专项：仍只操作客机

这组测试可能影响客机 SSH。先保留一个 **Mac→Linux 宿主** shell，用于从宿主 reset；不要只留下客机终端。所有 `sudo`、`ip`、服务和配置操作都必须在 `tester@clashtui-test` 中执行。

开始前结束其他业务用例，不再点击 Profile 激活/运行设置持久化。下文直接准备网络测试配置，测试后恢复原字节，不把手工预置的网络字段算作共享管理保存测试。

### M24：DNS 经独立回环上游，宿主/Mac DNS 不变

前提：夹具 D 运行，UDP 18053 已开启，TUN 仍关闭。

在 **客机 B**备份实际配置和网络状态：

```bash
cp "$CT_CONFIG/mihomo/config.yaml" "$CT_LAB/network-active-original.yaml"
chmod 600 "$CT_LAB/network-active-original.yaml"
ip -j rule > "$CT_LAB/network-rules-before.json"
ip -j route > "$CT_LAB/network-routes-before.json"
python3 - <<'PY'
from pathlib import Path
import json, yaml
p = Path('/home/tester/clashtui-test/config/mihomo/config.yaml')
v = yaml.safe_load(p.read_text())
v['dns'] = {'enable': True, 'listen': '127.0.0.1:15353', 'enhanced-mode': 'redir-host',
            'nameserver': ['udp://127.0.0.1:18053'], 'default-nameserver': ['127.0.0.1']}
p.write_text(json.dumps(v))
PY
/opt/clashtui/bin/mihomo -t -d "$CT_CONFIG/mihomo" -f "$CT_CONFIG/mihomo/config.yaml"
sudo systemctl restart clashtui-test-mihomo
ct core status
```

校验若失败，立即恢复 backup，再重启；不要继续测。

发出一个客机 UDP DNS A 查询：

```bash
python3 - <<'PY'
import socket, struct
query = struct.pack('!HHHHHH', 12345, 0x0100, 1, 0, 0, 0)
query += b'\x07example\x04test\x00' + struct.pack('!HH', 1, 1)
with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as client:
    client.settimeout(5)
    client.sendto(query, ('127.0.0.1', 15353))
    response, _ = client.recvfrom(4096)
assert response[:2] == query[:2]
assert socket.inet_aton('203.0.113.10') in response
print('PASS: Mihomo DNS answered 203.0.113.10 via local upstream')
PY
```

观察：15353 由客机 Mihomo监听，上游为夹具18053；响应含指定测试地址。失败时保留 timeout/错误、客机 journal 和 `ss -lun`，确认夹具 D 未退出。此测试没有修改任何系统 `/etc/resolv.conf` 或 Mac DNS。

恢复 DNS 测试状态：

```bash
cp "$CT_LAB/network-active-original.yaml" "$CT_CONFIG/mihomo/config.yaml"
sudo systemctl restart clashtui-test-mihomo
ct core status
```

### M25：TUN 真实接口、开关同步和自动策略路由

1. 保留 M24 的原配置与原 `ip rule`。当前核心已恢复、DNS 关闭。
2. 在 **客机**赋予测试服务必要能力，只创建本轮独立 drop-in：

```bash
sudo mkdir -p /etc/systemd/system/clashtui-test-mihomo.service.d
printf '[Service]\nAmbientCapabilities=CAP_NET_ADMIN CAP_NET_BIND_SERVICE\n' | sudo tee /etc/systemd/system/clashtui-test-mihomo.service.d/manual-test.conf >/dev/null
sudo systemctl daemon-reload
python3 - <<'PY'
from pathlib import Path
import json, yaml
p = Path('/home/tester/clashtui-test/config/mihomo/config.yaml')
v = yaml.safe_load(p.read_text())
v['tun'] = {'enable': False, 'device': 'ct-manual', 'stack': 'gvisor', 'auto-route': False,
            'auto-detect-interface': False, 'inet4-address': ['10.88.0.1/30']}
p.write_text(json.dumps(v))
PY
sudo systemctl restart clashtui-test-mihomo
```

3. 在 TUI Settings 选 Tun Enable 并开启；或管理页临时 JSON `{"tun":{"enable":true}}`。另一端与 CLI 状态应显示 true。客机 `ip -j link` 应出现实际 `ct-manual`，不能只凭 UI 开关颜色判成功。
4. 关闭后 true→false，实际接口应撤销或按核心行为停止使用；保存相应观察与 journal。无权限时明确报失败，不应伪装开启。
5. 自动路由单独用完整配置预置，开启并重启：

```bash
python3 - <<'PY'
from pathlib import Path
import json, yaml
p = Path('/home/tester/clashtui-test/config/mihomo/config.yaml')
v = yaml.safe_load(p.read_text())
v['tun'].update({'enable': True, 'auto-route': True, 'auto-detect-interface': True})
p.write_text(json.dumps(v))
PY
sudo systemctl restart clashtui-test-mihomo
ip -j link
ip -j rule
ip -j route
ct core status
```

预期：ct-manual 真实存在，核心 true，客机新增策略规则/路由。此处证明接口与策略创建，不自动证明所有公网流量都走正确代理；实际公网 TUN 流量另在联网专项验证。

6. 恢复原配置，移除**仅本轮** drop-in：

```bash
cp "$CT_LAB/network-active-original.yaml" "$CT_CONFIG/mihomo/config.yaml"
sudo rm -f /etc/systemd/system/clashtui-test-mihomo.service.d/manual-test.conf
sudo systemctl daemon-reload
sudo systemctl restart clashtui-test-mihomo
ip -j link
ip -j rule > "$CT_LAB/network-rules-after.json"
diff -u "$CT_LAB/network-rules-before.json" "$CT_LAB/network-rules-after.json"
ct core status
```

预期 Tun Enable false、ct-manual 消失、策略规则与原记录一致、7890/API 可用。DNS 15353 不再监听。客机 SSH 如果中断，**从 Linux 宿主**执行第 11 节基线恢复，不在 Mac 开启系统代理“修复”。

## 10. 联网、升级、平台与持续运行：独立扩展测试

### M26：公网订阅 / 远程节点

默认 VM 阻断外连，公网测试失败不能直接记为功能 FAIL。需要时先导出离线记录，在 **Linux 宿主**正常关机并显式允许客机联网：

```bash
bash scripts/vm.sh stop
bash scripts/vm.sh start --internet
bash scripts/vm.sh wait --seconds 45
bash scripts/vm.sh tunnel
```

Mac 外层隧道可保留，但浏览器需重新连接；客机 shell 也要重连。此模式允许客机访问可路由的公网、宿主/LAN 地址，仍不建立网桥或改变宿主默认路由。

1. 使用专门测试订阅与远程节点，导入后先 Check，再激活。三端核对节点、额度、更新错误和缓存。
2. 用**客机显式代理**连接一个自己控制的 HTTPS 回显服务，记录出口 IP、返回码、TLS/HTTP结果；DIRECT 与真实代理应有可解释差异，不能只以测速成功判路由正确。
3. 分别测错误凭据、订阅到期、302/HTTPS、限速/超时、短暂断网再恢复；旧缓存保留与错误结果对照。
4. TUN 公网流量应再验证 DNS、TCP/UDP 与停用后的恢复。Mac 浏览器默认仍只用于控制页面，不自动经过客机代理，故不能用 Mac 的普通公网访问判断客机出口。
5. 所有截图与日志脱敏订阅 URL 查询参数、用户名/密码、节点认证；记录核心与协议版本。

结束后正常关闭、以 `start`（不带 --internet）恢复默认阻断；最后 `check` 在干净基线确认。

### M27：面板部署、核心/Geo 升级及失败恢复

这些会替换客机文件，放在一轮独立的可 reset 环境进行，不与基础用例混跑。

| 操作 | 手工步骤 | 观察与核对 |
|---|---|---|
| 面板部署 | 联网后管理页“部署锁定版本面板”，先取消，再确认；CLI `ct panel` 独立核对；TUI Status `i` | 下载/摘要验证成功后部署 v1.273.1；目标版本文字与安装记录分别核对；清理浏览器缓存后重新加载真实资源 |
| 核心升级 | 记录核心版本、可执行文件摘要、有效配置；在独立测试轮次运行 `ct update mihomo`（有更新时直接替换）；核心 API `ct core maintenance upgrade --yes` 作为独立入口 | 目标架构正确，压缩归档已解包；升级后版本、Check、API、Profile激活和真实连接都通过 |
| Geo 更新 | 先配置有实际 GEO 规则的专用 Profile，运行 `ct core maintenance upgrade-geo --yes` 或面板对应入口 | 文件更新与真实规则命中可核对；只下载成功不等于规则路由验证通过 |
| 缓存清理 | 面板对应 DNS/fake-IP 清理；CLI `ct core maintenance flush-dns --yes`、`flush-fakeip --yes`；TUI Service/Settings 若提供对应动作按帮助执行 | 成功/不支持清晰；没有启用相关功能时记录范围限制，不能声称数据效果已验证 |
| 升级失败 | 在测试环境制造不可达下载源/明确失败，记录原摘要和服务状态 | 不应留下半写入可执行文件；可恢复原构建/配置，错误明确；源注入、危险归档已由隔离单元覆盖，实际故障另记 |

接口或发布资产不提供当前平台可验证摘要时，应明确拒绝而不是任意下载。“无可用更新”是检查结果，不等于完成过真实升级。记录实际是否发生文件替换、服务是否重启；失败后从基线恢复。

`ct update mihomo` 当前有更新时会直接下载并替换配置中的核心二进制，并不保证出现确认框；只有在准备好的独立客机测试轮次执行。核心 API 升级是另一个入口，二者的校验与回读分别记录。

### M28：控制地址 / 密钥迁移与远程端点边界

**在线激活**修改 control endpoint/secret 应拒绝，并保留当前文件。真正迁移按停服流程测试：

1. 关闭所有 TUI、管理页面操作及客机管理服务；保存实际 config.yaml 与覆盖配置的私有备份。
2. 停止客机核心，在这两份配置中同步改控制端口/secret；核心 -t 校验。
3. 启动核心，再启动新进程 CLI/TUI/管理服务，核对新的认证/身份。
4. 控制端口若改变，Linux→客机隧道也要更新目标；旧 `vm.sh tunnel` 只支持既定 9090。不得只改 MetaCubeXD 端点就假定三端已迁移；本用例需自行建立新的回环 SSH -L，并记录映射。
5. MetaCubeXD 更新端点/secret后能读写；旧 secret/旧端口不再能控制。普通 Profile 激活仍通过。
6. 失败则停服，同步恢复两份文件及原隧道映射，再重新启动全部客户端。不要在一份恢复、一份未恢复的状态反复激活。

远程端点边界需要**另一个独立测试控制实例**：允许节点/连接等核心 API 读取和操作；本机服务控制、实际本地配置激活应拒绝。当前本文基础拓扑的核心和业务层在同一客机，两个 SSH 隧道本身不构成“客户端配置指向远程核心”的验证。

### M29：Mac 原生 / Windows 原生平台专项

Mac 用浏览器访问 Linux VM 没有覆盖：Mac 二进制运行、launchd 服务、Mac 原生外部编辑器/`pbcopy`/`open`。需要这些结果时在独立 macOS 测试环境安装匹配架构构建、独立配置和 launchd 测试服务，再重复本文业务用例；使用正常主机前先安排可恢复的独立环境。

Windows 还需独立 VM 验证服务注册/卸载、系统代理开关与恢复、路径/编辑器/剪贴板、安装脚本。Linux 的安装/卸载、自启和重启恢复也需独立测试；当前 VM 服务启动/停止通过不等于安装与重启自启生命周期已验收。

远端三平台 CI 运行结果单独附上。操作系统差异和能力跳过分开记录，不将 Mac 浏览器 PASS 填成 macOS 服务 PASS。

### M30：持续运行、规模和浏览器交互稳定性

1. 使用专门大规模 Profile（例如 500/5000 节点、10000 规则），记录输入规模、初始内存及页面载入耗时。不要向测试订阅混入真实密钥。
2. 分别在 Chrome、Safari 操作：真实鼠标点击、键盘焦点、滚动、缩放、文件下载、剪贴板和确认框；重复点击容易被 2 秒刷新替换的行按钮，记录丢失点击/焦点，而不使用自动化 DOM click 代替人工结论。
3. TUI 搜索、筛选、跳转、缩放；在多次更新/服务断线后仍能操作，不持续增加后台任务。
4. 保持运行 1～2 小时，每隔 10 分钟在客机记录 RSS/FD：

```bash
ps -C clashtui -o pid,etime,rss,args
ps -C mihomo -o pid,etime,rss,args
```

对选定 PID 使用 `ls /proc/实际PID/fd | wc -l`。日志/图表缓冲应有限；持续单调增长需记录趋势，不能凭单个峰值判断泄漏。

5. 多页面并发、TUI+CLI 并发保存/更新，确认有冲突或忙碌反馈而非静默丢失。服务反复重启后连接与日志能够恢复，不成倍重复。
6. 大规模/长时测试用例参数、耗时和资源曲线随报告附上。小规模基础用例通过不能替代这项结果。

## 11. 证据导出、恢复与结束

### 11.1 保存可分享的证据

在 **客机 B**建立一次性报告目录；不要把整个 config 目录打包，它包含 secret/令牌和可能的真实订阅：

```bash
export CT_RUN=$(date -u +%Y%m%dT%H%M%SZ)
export CT_EVIDENCE="$CT_LAB/evidence-$CT_RUN"
mkdir -m 700 "$CT_EVIDENCE"
ct --version > "$CT_EVIDENCE/clashtui-version.txt"
/opt/clashtui/bin/mihomo -v > "$CT_EVIDENCE/core-version.txt"
ct manage state > "$CT_EVIDENCE/state.json"
ct core status > "$CT_EVIDENCE/status.json"
systemctl is-active clashtui-test-mihomo clashtui-test-web > "$CT_EVIDENCE/services.txt"
journalctl -u clashtui-test-mihomo -u clashtui-test-web --since '1 hour ago' --no-pager > "$CT_EVIDENCE/journal.txt"
```

选择性复制本轮 connections/metrics/logs、错误诊断、摘要对照。不要复制 `management-token`、SSH 私钥、含 secret 的网络备份、完整 `.web-task.json` 或整份覆盖配置。

分享前做凭据文本替换，并人工复查订阅 URL、节点认证及浏览器截图。可在客机对已挑选的文本证据脱敏：

```bash
python3 - <<'PY'
from pathlib import Path
import os, yaml
root = Path('/home/tester/clashtui-test/config')
directory = Path(os.environ['CT_EVIDENCE'])
secrets = [(root/'management-token').read_text().strip(),
           yaml.safe_load((root/'mihomo/core_override_config.yaml').read_text())['secret']]
for p in directory.iterdir():
    if p.is_file():
        text = p.read_text()
        for value in secrets:
            if value:
                text = text.replace(value, '[redacted]')
        p.write_text(text)
PY
tar -czf "$CT_LAB/evidence-$CT_RUN.tgz" -C "$CT_LAB" "evidence-$CT_RUN"
```

这个替换只处理两种本地凭据，**不自动脱敏任意公网订阅参数**；真实订阅证据仍需人工处理。

复制到 **Mac 本地**，将实际报告文件名填入：

```bash
export CT_LINUX='yulinye@实际Linux地址'
ssh "$CT_LINUX" 'cd /home/yulinye/clashtui && bash scripts/vm.sh ssh "cat /home/tester/clashtui-test/manual/evidence-实际时间.tgz"' > "$HOME/Desktop/clashtui-manual/evidence-实际时间.tgz"
```

确认归档可读取后才 reset。截图、手工记录模板和浏览器版本由 Mac 另存；不要把脱敏前归档分享出去。

### 11.2 正常恢复

1. 停止流量 E、关闭 TUI C、Ctrl-C 停止夹具 D。若做过网络专项，先执行对应原配置/drop-in 恢复并核对。
2. 导出证据；然后在 **Linux 宿主**执行第 2 节 stop/reset/start/wait/push、客机权限修正与管理服务重启、tunnel/check。
3. `check` 应显示 baseline、两个服务 active、DNS/TUN false、默认外连阻断。reset 后夹具目录消失，下一轮需重新准备。
4. Mac 隧道 A 最后 Ctrl-C 关闭；本地页面此后应断线。Mac 没有系统代理/TUN/DNS 需要清理。
5. 核对宿主原路由、DNS 摘要及原有服务状态，与测试前记录比较；不要把宿主本来就存在的 TUN 当成测试新建接口。

对照命令（分别在对应机器执行）：

```bash
# Linux 宿主
cd /home/yulinye/clashtui
ip -j route > target/manual-results/host-routes-after.json
sha256sum /etc/resolv.conf > target/manual-results/host-dns-after.txt
systemctl is-active mihomo.service > target/manual-results/host-existing-service-after.txt
diff -u target/manual-results/host-routes-before.json target/manual-results/host-routes-after.json
diff -u target/manual-results/host-dns-before.txt target/manual-results/host-dns-after.txt
diff -u target/manual-results/host-existing-service-before.txt target/manual-results/host-existing-service-after.txt
```

```bash
# Mac 本地（CT_MAC_DIR 需按第3.3节设置）
scutil --proxy > "$CT_MAC_DIR/mac-proxy-after.txt"
scutil --dns > "$CT_MAC_DIR/mac-dns-after.txt"
route -n get default > "$CT_MAC_DIR/mac-default-route-after.txt"
diff -u "$CT_MAC_DIR/mac-proxy-before.txt" "$CT_MAC_DIR/mac-proxy-after.txt"
diff -u "$CT_MAC_DIR/mac-dns-before.txt" "$CT_MAC_DIR/mac-dns-after.txt"
diff -u "$CT_MAC_DIR/mac-default-route-before.txt" "$CT_MAC_DIR/mac-default-route-after.txt"
```

### 11.3 异常恢复

- **只有页面断线**：检查 Mac A、Linux tunnel、客机服务，按第 12 节分层排查，不先 reset。
- **核心配置失败但客机 SSH 正常**：恢复客机私有备份，重启 `clashtui-test-mihomo`，再读取 API；保留失败日志。
- **客机 SSH 因网络测试不可达**：Mac SSH 到 Linux 宿主仍应可用。先保存宿主的 VM 串口日志 `target/vm/console.log`，用 `vm.sh stop` 正常关闭，再 reset；无法 SSH 时先不要假定能导出客机内文件。
- **stop 超时**：脚本保留运行状态并报错。记录 QEMU/串口状态，不删除正在运行的 qcow2，不同时启动第二个实例。基线恢复属于明确丢弃本轮客机数据的操作，应先确认取证需要。

## 12. 常见故障：按层定位

| 现象 | 检查顺序 | 预期 / 处理 |
|---|---|---|
| Mac SSH Linux 失败 | SSH 用户/IP、Linux SSH 服务与网络、主机指纹 | 这是宿主访问问题，尚未测试业务；不开放核心 API 到 LAN 解决 |
| 浏览器连接被拒绝 | Mac A 是否仍运行 → Linux `vm.sh status/tunnel` → 客机两服务 | Mac 转发和 Linux→VM 转发都要存在 |
| 隧道启动端口占用 | Mac lsof；Linux 上是否已有本轮 SSH tunnel | 复用本轮隧道或换 Mac 本地端口；不杀其他服务 |
| 401/403 | 管理页 token 与 MetaCubeXD secret 是否互换，是否含多余空白 | 两种认证独立；不要修改 secret 绕过认证 |
| 静态面板可开，节点却读不到 | MetaCubeXD 端点是否为 Mac `http://127.0.0.1:39090`、核心是否在线、secret | 面板 HTML 成功不等于 API 认证成功 |
| 管理页打开面板链接指向 9090 | 链接来自客机真实控制端点 | 本拓扑手动开 Mac 39090，不修改核心控制地址 |
| 本地夹具订阅连接失败 | 客机 D 是否运行、客机 B `/hello`、URL 是否错填 Mac 39090/39091 | URL 应是客机18080；文件导入来自Mac本地 |
| 公网测速/健康检查失败 | VM restrict 状态、测速 URL | 默认离线；用本地probe验证，公网另轮测试 |
| TUI 显示权限修复提示 | 目录组权限、基线版本 | 在客机按第2节修正，不更改宿主目录权限 |
| TUI 乱码/键盘无响应 | 两层SSH -tt、TERM、终端尺寸、是否误进vi/raw-mode冲突 | 重连终端；先独立导航测试，保留尺寸与按键记录 |
| 剪贴板/打开浏览器/编辑器失败 | 动作实际在Linux客机执行，是否具备GUI/工具 | Mac终端不能自动代办；按M23或平台专项记录 |
| 修订冲突 | 真并发保存、后台Profile变更、已打开文档未重读 | 保留未保存内容后关闭/重读；只读CLI场景不应冲突 |
| Web 操作忙碌 | `/api/job` pending、订阅超时、进程锁 | 不连续点击；任务结果应仍可轮询；必要时按M22验证中断恢复 |
| TUN开启失败 | 客机服务capabilities、/dev/net/tun、journal | 不能只看开关；权限准备只针对客机测试服务 |
| DNS查询超时 | 客机15353、夹具UDP18053、配置校验与journal | 不改Mac或宿主DNS；恢复实际config备份 |

## 13. 本轮最终结论如何写

分别记录 mini PC 自动报告、仍待自动化的子项、H01–H06 人工体验、公网与原生平台专项。例如：“Linux VM 自动报告见某目录；Mac Chrome H01/H02/H03/H04 通过，H05 依赖未准备，H06 Safari 未执行；M08 认证/超时完整业务断言待补。”

不再要求人工完成 M01～M30 后才给结论。未执行、BLOCKED、SKIP、失败和自动化通过分别统计，不得将某编号的部分自动覆盖算作全部子项通过。FAIL 应对应具体动作、预期与实际、构建版本、日志/截图和恢复结果。
