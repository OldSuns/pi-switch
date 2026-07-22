# pi-switch

一个直接维护 Pi 官方配置文件的终端 TUI：管理 provider、模型和默认模型，不创建第二份 provider 配置。

## 使用

```bash
npm install
npm run build:native:debug
node ./bin/pi-switch.js
```

也可以运行：

```bash
node ./bin/pi-switch.js --version
node ./bin/pi-switch.js doctor
```

`n/e/d/c` 会根据当前焦点新建、编辑、删除、复制 provider 或 model。方向键切换栏目和选择项目，`Enter` 进入模型列表，`Esc` 返回 provider，`i` 在线导入模型。Settings 中可切换 English/中文、开关 pi.dev 实时模型信息、编辑关闭实时信息时使用的默认参数；选择 `Import from OpenCode` 后可全选或勾选部分 provider，再从 `~/.config/opencode/opencode.json` 导入。`Space` 设置默认模型，`b` 恢复备份，`v` 检查配置，`?` 查看全部键位。

Provider 表单支持可选 `baseUrl`、`api`、`apiKey`、`authHeader`、`headers` 和 `compat`；model 表单支持 `id`、`name`、API override、reasoning、文本/图像输入、context window 和 max tokens。`cost`、`thinkingLevelMap`、`modelOverrides`、OAuth 和其他未知字段会无损保留。字段语义以 [Pi Custom Models](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/models.md) 为准。

开启 `Fetch model metadata from pi.dev` 后，在线模型导入与 OpenCode 导入每次都会请求 `https://pi.dev/api/models`，用实时目录补全 `contextWindow`、`maxTokens` 和 `cost`。自定义 provider 仅在 model ID 的元数据唯一时自动匹配；有歧义或未收录的模型会明确计数。关闭后不会请求 pi.dev，而是写入 Settings 中配置的默认参数；空字段使用 Pi 官方默认值（context window `128000`、max tokens `16384`、cost `0`）。

## 数据安全

- 唯一事实来源是 `~/.pi/agent/models.json` 与 `settings.json`。
- OpenCode 配置只读；导入结果通过同一套校验、备份和原子写入流程合并到 Pi 配置。
- 修改前会备份到 `~/.pi-switch/backups/`，每类保留最近 20 份。
- 写入只 patch 目标字段，保留未知 JSON 字段；格式错误时停止写入并显示错误。
- 支持 Pi 的 `$ENV` / `${ENV}` 插值与 `$$` / `$!` 转义；`!command` 原样保存，但在线拉取不会执行它。
- 正常退出、错误和 panic 都会恢复 raw mode、alternate screen 与光标状态。

## 验证

```bash
cargo test --locked --lib
cargo fmt -- --check
cargo clippy --locked --all-targets -- -D warnings
npm run build:native:debug
npm run pack:check
```
