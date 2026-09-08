# Web 操作界面

对应 [issue #3](https://github.com/OldSuns/pi-switch/issues/3)：`pi-switch --web`。

## 设计

沿用 TUI 的主页、配置、会话、设置导航，以及 provider → 模型的主从关系。provider 是否保存在本地、是否同步到 Pi、哪个模型是默认模型，分别展示。主页围绕可直接切换的默认模型、Provider 状态、可进入的最近会话和配置工具组织，不展示静态流程说明卡片。

「外观」提供 Catppuccin Mocha 暗色与 Latte 亮色，浏览器保存主题偏好。Mauve 为主色，Base / Mantle / Surface 区分内容、导航和面板，绿色表示已同步，黄色表示默认或需注意，红色表示删除和错误。颜色统一定义在 `public/styles.css`，所有页面、表单、模态及 Markdown 都随主题切换。

操作按钮有明确文字或可访问名称；表单保留错误和输入内容，只有实际保存成功才更新配置视图。默认 provider 取消同步、删除配置/会话、恢复备份和安装更新均有确认。原生 `dialog` 提供焦点约束及 Escape 关闭；导航、列表、搜索和表单可用键盘操作。

## 实现

- `public/`：HTML、CSS、原生 JavaScript 模块，无前端运行时依赖和独立打包步骤。
- `server.js`：Node HTTP 传输，只公开打包的静态资源；API 接受同源 JSON POST，并验证 Host、Origin、请求大小和非有限数值。
- `native-client.js` / `native-worker.js`：持久 Worker 实例，避免原生网络操作阻塞 HTTP 事件循环。原生失败明确返回，Worker 异常后拒绝后续请求。
- `src-rust/web/`：输入/输出适配、服务器端导入计划、会话 Markdown 渲染。所有文档操作复用 `documents`，不实现第二套文件读写业务逻辑。

每个 Worker 构造独立的原生 `WebSession`，使用与 TUI 相同的配置路径发现规则。

## 验证

```bash
cargo test --locked --lib
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
npm run build:native:debug
npm run pack:check
```

Rust 单元测试覆盖配置同步、默认模型、未知字段保留、输入边界、Session 文件操作及 Markdown 安全渲染。后端测试执行时应设置 60 秒硬超时；原生编译单独进行。
