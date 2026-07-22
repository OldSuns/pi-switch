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

`n/e/d/c` 会根据当前焦点新建、编辑、删除、复制 provider 或 model。方向键切换栏目和选择项目，`Enter` 进入模型列表，`Esc` 返回 provider，`i` 在线导入模型，`Space` 设置默认模型，`b` 恢复备份，`v` 检查配置，`?` 查看全部键位。

Provider 表单支持可选 `baseUrl`、`api`、`apiKey`、`authHeader`、`headers` 和 `compat`；model 表单支持 `id`、`name`、API override、reasoning、文本/图像输入、context window 和 max tokens。`cost`、`thinkingLevelMap`、`modelOverrides`、OAuth 和其他未知字段会无损保留。字段语义以 [Pi Custom Models](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/models.md) 为准。

## 数据安全

- 唯一事实来源是 `~/.pi/agent/models.json` 与 `settings.json`。
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
