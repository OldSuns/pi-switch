# pi-switch

一个维护本地 provider 库并按需同步 Pi 官方配置文件的终端 TUI：管理 provider、模型、默认模型，以及 `~/.pi/agent/sessions/` 下的 session 列表/预览/删除。所有 provider 完整保存在 `~/.pi-switch/providers.json`，只有已启用项会写入 Pi 的 `models.json`。

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

`n/e/d/c` 会根据当前焦点新建、编辑、永久删除、复制 provider 或 model。Provider 列表中的 `[x]` 表示已加入 Pi，`[ ]` 表示仅保存在本地；在 provider 焦点按 `Space` 添加/移除 Pi，在 model 焦点按 `Space` 设置默认模型。新建 provider 默认加入 Pi，也可在表单关闭“加入 Pi”；复制继承源状态。方向键切换栏目和选择项目，`Enter` 进入模型列表，`Esc` 返回 provider，`i` 在线导入模型。Sessions 页可浏览全部项目 session、预览 user/assistant 文本、`/` 筛选、`n` 仅命名、`u` 预览仅用户消息、`d` 删除（优先 trash）。Settings 中可切换 English/中文、开关 models.dev 实时模型信息、编辑关闭实时信息时使用的默认参数；选择 `Import from OpenCode` 后可全选或勾选部分 provider，再从 `~/.config/opencode/opencode.json` 导入，导入项默认加入 Pi。`b` 恢复备份，`v` 检查配置，`?` 查看全部键位，`q` 退出 TUI。

## 会话管理

Sessions 页读取 Pi 的 session JSONL：优先使用 `PI_CODING_AGENT_SESSION_DIR`，否则使用 `PI_CODING_AGENT_DIR` 下的 `sessions/`，默认 `~/.pi/agent/sessions/`。列表显示 session 标题、消息条数和修改时间；黄色标题表示手动命名的 session，白色标题表示使用第一条用户消息作为标题。

- `/` 筛选 session，`n` 只看命名 session，`r` 重新扫描磁盘。
- `Enter` / `Right` 进入右侧预览；预览中 `Up` / `Down` 按消息切换，鼠标滚轮按行滚动长消息。
- `u` 只显示用户消息；`Ctrl+C` 复制当前选中消息内容。
- `d` 删除当前 session，优先调用系统 trash，失败后再永久删除文件，删除前会确认。

Provider 表单支持可选 `baseUrl`、`api`、`apiKey`、`authHeader`、`headers` 和 `compat`；Headers 弹窗提供单独的 `User-Agent` 输入，其他请求头继续使用 JSON 编辑器，保存后仍写入 `headers["User-Agent"]`。model 表单支持 `id`、`name`、API override、reasoning、文本/图像输入、context window 和 max tokens。`cost`、`thinkingLevelMap`、`modelOverrides`、OAuth 和其他未知字段会无损保留。字段语义以 [Pi Custom Models](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/models.md) 为准。

开启 `Fetch model metadata from models.dev` 后，在线模型导入与 OpenCode 导入会请求 `https://models.dev/api.json`，用实时目录补全 `contextWindow`、`maxTokens` 和 `cost`。自定义 provider 的 model ID 若匹配到多个不同价格，会先列出来源 provider 供用户选择；未收录或缺少有效 token 上限的模型会明确计数。关闭后不会请求 models.dev，而是写入 Settings 中配置的默认参数；空字段使用 Pi 官方默认值（context window `128000`、max tokens `16384`、cost `0`）。

## Provider 库与 Pi 同步

- `~/.pi-switch/providers.json` 是 pi-switch 的完整 provider 库；`~/.pi/agent/models.json` 只包含当前加入 Pi 的子集。
- 首次运行会把现有 `models.json` provider 全量导入本地库，不修改 Pi 配置。
- 已加入 provider 的编辑和 model 变更会同步到两份文件；仅本地项只更新本地库，在线导入 model 不会隐式加入 Pi。
- 每次启动或手动重载以 `models.json` 中的同 ID provider 为准回灌本地库；外部从 Pi 删除的 provider 仍作为仅本地项保留。
- 从 Pi 移除当前默认 provider 会先确认并清除默认模型；永久删除 `d` 则删除本地副本，并在必要时同时从 Pi 删除。

## 数据安全

- Provider 的完整事实来源是 `~/.pi-switch/providers.json`；Pi 启用子集是 `~/.pi/agent/models.json`。
- OpenCode 配置只读；导入结果通过同一套校验、备份和原子写入流程合并到 Pi 配置。
- Session 删除只作用于选中的 JSONL 文件，并校验路径必须位于 Pi session 目录内；优先移入系统回收站。
- 修改前会备份 `providers.json`、`models.json` 与 `settings.json` 到 `~/.pi-switch/backups/`，最多保留最近 10 份。旧版双文件备份不支持恢复。
- `providers.json` 损坏时会归档原文件并从当前 Pi 配置重建，启动时显示归档路径。
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
