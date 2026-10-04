# Vue 管理面板

范围：替换当前 MetaCubeXD 核心面板和 ClashTui 本地管理功能。不包括上游可选 agent 的 WebDAV、script/merge、桌面壳及多端点档案。

## 架构与构建

前端位于 `web/src/`，使用 Vue 3、TypeScript、Vite、Vue Router、Pinia、Lucide 图标和 ECharts。路由采用 hash，前端资源编译成 `web/dist/index.html` 单文件并嵌入 Rust 可执行文件，用户运行面板不需要 Node、CDN 或独立静态站点。

```bash
npm ci --prefix web
npm run build --prefix web
npm test --prefix web
node web/scripts/build-manifest.mjs --check
cargo build --locked --all-features
```

`web/dist/manifest.json` 记录源码/锁文件及 HTML 摘要，检查失败时必须重新构建前端。提交前端源代码时一并提交 dist。Rust 单独构建使用仓库已生成资源；流水线先构建前端再构建 Rust。开发时 `npm run dev --prefix web` 将 `/api` 转发到回环 9091，仍需独立启动 `clashtui web`。

## 数据与认证

入口默认 `http://127.0.0.1:9091/`，由 `clashtui web --token-file /path/to/token` 提供。只接受独立管理令牌；认证成功后将令牌保存在当前标签页的 sessionStorage，刷新时自动重新认证；认证成功前不显示业务导航。管理令牌输入框仅在登录页显示；登录后各业务页面不保留凭据区域，顶部保留状态、刷新与退出入口。令牌以 Authorization 头发送，不写 URL 或 localStorage。localStorage 仅保存主题、密度、刷新周期和测速偏好。退出、令牌更改或 401 会移除已保存令牌；令牌更改/401 也会清空会话、草稿和缓存。浏览器禁用会话存储时仍可登录，但刷新后需重新输入。

- `/api/state`、`/api/action`、`/api/job` 保留文件事务、后台任务和恢复记录。
- `/api/core/read` 只开放列举的核心查询，使用 4 个读取线程和 16 项有界等待队列；超载返回 503。状态、任务查询和操作提交保留独立分派通道。核心修改通过后台任务的 `action=core` 分派。
- 运行设置按编辑基线生成差异，只提交用户修改的字段及 `expected` 原值；后台在应用前检查这些字段是否冲突。Mihomo 不提供原子条件 PATCH，此检查属于应用前校验，不能替代核心级原子事务。
- 日志缓存代次或认证会话变化时，页面重置清空位置和暂停快照，恢复展示新缓存。
- `src/functions/web/core.rs` 复用 CoreSession、config、proxies、resources、connection、cache、geo 和 control；不是任意 URL 代理。
- Rust 通过认证头接入 traffic/memory/connections/logs 四路 WebSocket，断线重连后由浏览器轮询共享缓存。日志保留 1000 条，长日志截断；前端采样保留 120 点。离线指标为缺失值，保留已采集历史，不伪造零速率。
- 核心 secret 留在 Rust，运行配置查询去除凭据与控制地址，日志中递归遮蔽凭据。
- 连接关闭捕获固定 ID；模式改变时可显式关闭修改前捕获的连接。更新、健康检查和关闭批次逐项报告失败。

## 页面与兼容

总览、订阅与配置、模板、节点与分组、Provider、规则、连接、日志、设置、服务与维护覆盖当前核心和本地管理范围。业务入口见[三端矩阵](../reference/cli_tui_web_parity_zh.md)。核心未暴露的字段不开放修改，规则 disabled 字段缺失时不提供开关。

默认配置不再设置 external-ui/external-ui-url。已有用户配置中的旧字段不会自动删除；旧资源可保留，但新面板和测试不再依赖它们。`clashtui panel` 和 `manage prepare_panel` 兼容保留，改为检查内置资源，不下载或替换目录。TUI Status 的 w 打开 9091，i 检查内置面板；自定义浏览器地址可设置 `CLASHTUI_WEB_URL`。更换 Web 监听端口时需相应设置该地址。

确认和输入统一使用站内 `ActionDialog`，跟随主题和手机布局；原生 HTML dialog 管理模态焦点，支持 Esc/取消，危险操作使用红色确认按钮。对话框本身不提交业务请求，确认后才执行；路由或认证会话变化会关闭待确认内容。

## 上游参考与验证

参考固定上游提交 `8bbc8f58fef71148a94fb5c0ff808f79b057337d` 的 useApi/useQueries 及此前[协议审计](../archive/README.md#upstream)，核对节点选择、自动组恢复、测速、Provider、规则、连接、配置及维护接口。此次自行实现 Vue 组件和交互，没有引入上游发布包作为运行资源。

模拟浏览器验证点击、错误、跨端回读、认证、资源能力、固定连接集合、实时数据、重连和日志边界；真实 VM 验证实际 Mihomo、服务及事务。报告只能证明对应构建和环境；Mac/Safari、真实剪贴板和终端体验仍单独观察。[测试方案](../testing/README.md)说明自动和人工边界。

## 视觉效果与可访问性

背景采用渐变光晕与点阵，标题区域提供 SVG 轨道装饰。数据卡片和节点卡片提供鼠标位置柔光，页面切换与弹窗采用短时过渡；切换路由时保留 KeepAlive 内的草稿状态。装饰层设置 `pointer-events: none`，不参与焦点导航。无需远程素材或额外动画依赖。

系统 `prefers-reduced-motion: reduce` 开启时，背景、轨道、入场、弹窗和路由动效关闭，同时停止鼠标柔光坐标更新。窄屏减小装饰范围，触屏不启用悬停抬升。调整视觉效果后应检查桌面、窄屏、深色模式，以及减少动态效果下的页面切换。

添加订阅可选择直连或通过代理，默认直连；代理使用 ClashTui 配置的代理地址，需要可用的代理服务。选择与订阅一起保存，后续更新沿用该方式，可通过订阅行的“代理更新”切换。代理下载失败会报告错误，不会静默回退直连，也不会登记未成功下载的订阅。

全局操作通知固定在视口右下方，执行进度、成功、失败不会随页面滚动离开视野，可手动关闭；覆盖所有页面与文件编辑器，结果弹窗上方也可见。实时页面加载错误首次出现或变化时同步到悬浮通知，避免轮询反复通知同一错误。订阅行继续保留就近的进度、成功和失败提示。
