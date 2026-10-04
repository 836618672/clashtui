# Mihomo 单核心迁移

项目现在仅管理 Mihomo（Clash.Meta）。CLI、TUI、内置 Vue 管理 Web围绕同一 Mihomo 后端工作。

## 接口变化

- 程序的 `--core`、管理动作 `select_core` / `switch_core`、`service switch` 已移除。
- TUI 和管理页移除核心选择和切换入口。服务启动、停止、重启均操作配置中的 Mihomo 服务。
- Profile、模板及覆盖配置统一使用 Mihomo YAML；原有 Mihomo 文件格式和目录保留。
- 安装脚本默认安装 Mihomo，兼容参数 `--core mihomo` / `-Core mihomo` 仍接受，其他核心选项会被拒绝。
- 控制 API 会验证 Mihomo 身份，拒绝不支持的后端。

## 旧数据处理

首次读取含其他核心数据或旧核心选择的数据库时，程序在同目录创建 `clashtui.db.before-mihomo-only`，保存原始字节。备份已存在时不会覆盖；无法创建备份时拒绝继续加载。

随后加载 Mihomo Profile 和当前选择。数据库再次保存时只写 Mihomo 数据；原始备份可用于人工恢复。此迁移不转换其他核心的配置、节点或模板。

旧 `config.yaml` 中额外的核心配置会被忽略；旧主题及按键文件中的额外核心段也被忽略。建议手动删除这些已无效的段落。

此源码调整不卸载电脑上已安装的程序或服务，也不删除现有配置目录。需要清理已安装软件时，先备份并核实服务状态，再按所在系统的卸载流程操作。

## 验证

隔离测试验证旧选择回退到 Mihomo、Mihomo Profile 及当前项保留、原数据库备份不被覆盖，以及移除后的 CLI 参数被拒绝。完整运行方法见[测试流水线](../testing/test_pipeline_zh.md)。真实安装、服务及代理网络功能仍在隔离 VM 中验收。
