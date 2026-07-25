# pi-switch

维护本地 provider 库并按需同步 [Pi](https://github.com/earendil-works/pi) 配置的终端 TUI：管理 provider / 模型 / 默认模型，以及 Pi session 的列表、预览与删除。

CLI：`pi-switch` · npm 包：`@oldsuns/pi-switch` · Node 薄壳 + Rust/napi 原生核心

## 功能

- **主页**：provider / model 计数、默认模型、关键路径
- **配置 (Profiles)**：本地 provider 库与 Pi 启用子集；新建 / 编辑 / 删除 / 复制；在线导入模型
- **会话 (Sessions)**：浏览 JSONL session、筛选、预览、复制消息、删除
- **设置 (Settings)**：语言、models.dev 元数据、默认参数、重载、校验、备份、OpenCode 导入
- 完整库保存在 `~/.pi-switch/providers.json`，只有已启用项写入 Pi 的 `models.json`

## 要求

- Node.js `>= 20`
- 预构建原生模块当前仅覆盖 Windows (msvc x64)——直接 `npm install -g` 即装即用
- 本地构建原生模块（仅开发者）需要 Rust 工具链与 `@napi-rs/cli`

## 快速开始

```bash
# 全局安装
npm install -g @oldsuns/pi-switch

# 打开 TUI
pi-switch
```

首次运行会自动把现有的 `~/.pi/agent/models.json` 全量导入本地库，**不修改** Pi 配置；随后即可在 Profiles 中勾选要加入 Pi 的 provider / model。

CLI：

```bash
pi-switch                 # 打开 TUI（等同 tui）
pi-switch tui
pi-switch doctor         # 校验配置与默认模型
pi-switch --version      # / -v
pi-switch --help         # / -h / help
```

Windows 上无需 Rust：预构建原生模块随 npm 包分发，`pi-switch` 开箱即用。本地开发见下文「开发者」。

## 开发者

从源码构建：

```bash
npm install
npm run build:native:debug
node ./bin/pi-switch.js
```

## 界面与快捷键

全局导航：`j/k` 或方向键移动；菜单中 `Enter` / `l` 进入内容，`h` / `Esc` / `Tab` 回菜单；`?` 帮助；`q` 退出（多数界面 `Ctrl+C` 也退出，会话预览除外）。

### 配置 (Profiles)

| 键 | 作用 |
|----|------|
| `n` / `e` / `d` / `c` | 新建 / 编辑 / 删除 / 复制当前焦点（provider 或 model） |
| `Space`（provider） | 加入 / 移出 Pi；`[x]` 已加入，`[ ]` 仅本地 |
| `Space`（model） | 设为默认模型（仅已加入 Pi 的 provider） |
| `i` | 从当前 provider 在线导入模型 |
| `/` | 筛选 provider |
| `Enter` / `l` | 进入模型列表 |
| `Esc` / `h` | 从模型回到 provider，或从 provider 回菜单 |
| `r` | 从磁盘重载配置 |
| `b` | 浏览备份 |
| `v` | 校验配置（doctor） |

新建 provider 默认加入 Pi，表单可关闭；复制继承源的加入状态。

Provider 表单：`baseUrl`、`api`（`openai-completions` / `openai-responses` / `anthropic-messages` / `google-generative-ai`）、`apiKey`、`authHeader`、Headers（独立 `User-Agent` + 其余 JSON）、`compat`（含一等开关 Session affinity = `sendSessionAffinityHeaders`）。

Model 表单：`id`、`name`、API override、reasoning、文本/图像输入、context window、max tokens。`cost`、`thinkingLevelMap`、`modelOverrides`、OAuth 等未知字段会无损保留。字段语义以 [Pi Custom Models](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/models.md) 为准。

### 会话 (Sessions)

| 键 | 作用 |
|----|------|
| `/` | 筛选 |
| `n` | 仅显示手动命名的 session（不是新建） |
| `r` | 重新扫描磁盘 |
| `u` | 预览仅用户消息 |
| `Enter` / `Right` | 进入右侧预览 |
| 预览 `Up` / `Down` | 按消息切换 |
| 滚轮 / `PageUp` / `PageDown` | 按行滚动长消息 |
| 预览 `Ctrl+C` | 复制当前消息到剪贴板（不退出） |
| `d` | 删除当前 session（确认后优先 trash） |

黄标题 = 手动命名；白标题 = 使用第一条用户消息。

Session 根目录优先级：`PI_CODING_AGENT_SESSION_DIR` → `PI_CODING_AGENT_DIR/sessions` → `~/.pi/agent/sessions/`。

剪贴板：Windows `clip`、macOS `pbcopy`、Linux `wl-copy` / `xclip` / `xsel`。

### 设置 (Settings)

| 项 | 说明 |
|----|------|
| 语言 | English / 中文（`en` / `zh-CN`） |
| 从 models.dev 获取模型信息 | 开关实时元数据（默认开） |
| 默认模型参数 | 仅关闭实时元数据时显示，用于导入缺省 |
| 重载配置 | 从磁盘重读 |
| 验证配置 | doctor |
| 浏览备份 | 恢复 version 2 备份 |
| 从 OpenCode 导入 | 只读导入 `opencode.json` |

`Enter` / `Space` 执行当前项。

## Provider 库与 Pi 同步

- `~/.pi-switch/providers.json` 是完整本地库；`~/.pi/agent/models.json` 只含当前加入 Pi 的子集。
- 首次运行把现有 `models.json` 全量导入本地库，不修改 Pi 配置。
- 已加入 provider 的编辑与 model 变更会同步两份文件；仅本地项只更新本地库。
- 在线导入 model **不会**隐式加入 Pi。
- 启动或手动重载时，以 `models.json` 中同 ID provider 为准回灌本地库；外部从 Pi 删除的 provider 仍作为仅本地项保留。
- 从 Pi 移除当前默认 provider 会先确认并清除默认模型；`d` 永久删除本地副本，必要时同时从 Pi 删除。

## 模型导入与价格

在线导入（Profiles 中 `i`）：

1. 按 provider 的 `api` / `baseUrl` / 鉴权请求模型列表。
2. **NewAPI 网关价格（best-effort）**：去掉 `baseUrl` 尾部 `/v1` 后依次尝试  
   - `GET /api/ratio_config`（可能含 `create_cache_ratio`）  
   - `GET /api/pricing`  
   成功则按 NewAPI 换算覆盖模型 `cost`（`1 USD = 500_000 quota`，每 1M tokens 成本 ≈ `ratio × 2` USD）；失败静默忽略。
3. 若开启 models.dev 元数据：请求 `https://models.dev/api.json`，补全 `contextWindow`、`maxTokens`、`cost`、reasoning 等。  
   - **在线导入**遇到同 model ID 多源歧义时：**自动取第一个候选**，不弹选择框；缺少可用元数据的模型跳过并提示计数。  
   - 导入列表会标注价格来源：`ratio_config` 或 `models.dev`。  
   - 网关价格在 catalog 元数据之上叠加。
4. 关闭实时元数据：使用 Settings 中的默认参数；空字段回落 Pi 官方默认（context window `128000`、max tokens `16384`、cost `0`）。

**OpenCode 导入**（Settings）：只读 `~/.config/opencode/opencode.json`，可全选或勾选 provider；导入项默认加入 Pi。若 models.dev 仍有歧义，**需要用户选择候选**。OpenCode 配置本身不会被修改。

## 配置路径与 Settings 字段

| 路径 | 角色 |
|------|------|
| `~/.pi-switch/providers.json` | 完整本地 provider 库（`version: 1`） |
| `~/.pi/agent/models.json` | 已加入 Pi 的 provider 子集 |
| `~/.pi/agent/settings.json` | 默认模型 + pi-switch 设置 |
| `~/.config/opencode/opencode.json` | OpenCode 只读导入源 |
| `~/.pi-switch/backups/` | version 2 备份（providers + models + settings），最多 10 份 |
| `~/.pi-switch/write.lock` | 写入互斥锁 |

`settings.json` 中与 pi-switch 相关的字段：

| 字段 | 含义 |
|------|------|
| `defaultProvider` + `defaultModel` | 默认模型（成对存在或同时缺省） |
| `piSwitch.language` | `en` \| `zh-CN` |
| `piSwitch.fetchModelMetadata` | 是否拉 models.dev（默认 `true`） |
| `piSwitch.modelDefaults` | 关闭实时元数据时的导入缺省（context / maxTokens / cost） |

## 数据安全

- 写前备份 `providers.json`、`models.json`、`settings.json` 到 `~/.pi-switch/backups/`（version 2）；最多保留最近 10 份。旧版双文件备份不支持恢复。
- 写入使用 `write.lock` 互斥；异常残留锁时 `doctor` 会提示。
- `providers.json` 损坏时归档为 `corrupt-providers-*.json`，再从当前 Pi 配置重建，启动时显示归档路径。
- 原子写入，只 patch 目标字段，保留未知 JSON；格式错误时停止写入并显示错误。
- 支持 Pi 的 `$ENV` / `${ENV}` 插值与 `$$` / `$!` 转义；`!command` 原样保存，在线拉取**不会**执行它。
- Session 删除只作用于选中的 JSONL，并校验路径必须位于 session 根目录内；优先调用系统 `trash`，失败后再永久删除。
- 正常退出、错误和 panic 都会恢复 raw mode、alternate screen 与光标。

## 验证

```bash
cargo test --locked --lib
cargo fmt -- --check
cargo clippy --locked --all-targets -- -D warnings
npm run build:native:debug
npm run pack:check
```

## 许可

MIT

## 致谢

感谢 [LINUX DO](https://linux.do) 社区的讨论与反馈。
