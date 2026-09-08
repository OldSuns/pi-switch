# Web 操作界面

对应 [issue #3](https://github.com/OldSuns/pi-switch/issues/3)：`pi-switch --web`。

## 设计

沿用 TUI 的主页、配置、会话、设置导航，以及 provider → 模型的主从关系。provider 是否保存在本地、是否同步到 Pi、哪个模型是默认模型，分别展示。主页围绕可直接切换的默认模型、Provider 状态、可进入的最近会话和配置工具组织，不展示静态流程说明卡片。

默认使用 Catppuccin Latte 亮色，「外观」也可切换为 Mocha 暗色；浏览器保存的主题偏好优先生效。Mauve 为主色，Base / Mantle / Surface 区分内容、导航和面板，绿色表示已同步，黄色表示默认或需注意，红色表示删除和错误。颜色统一定义在 `public/styles.css`，所有页面、表单、模态及 Markdown 都随主题切换。

操作按钮有明确文字或可访问名称；表单保留错误和输入内容，只有实际保存成功才更新配置视图。默认 provider 取消同步、删除配置/会话、恢复备份和安装更新均有确认。原生 `dialog` 提供焦点约束及 Escape 关闭；导航、列表、搜索和表单可用键盘操作。

复制模型时保留源模型的完整配置，再应用复制表单中的修改；扩展字段不会因 Web 表单未展示而丢失，源模型和默认模型不变。

Provider 和模型列表分别支持名称升序／降序、添加时间升序／降序和自定义顺序。名称按显示名称（未设置时使用 ID）排序，忽略大小写并按数字大小比较。选择「自定义顺序」后，可拖动窄手柄，以元素之间的直线确定插入位置，也可使用 `Alt + ↑ / ↓` 调整；搜索或筛选时只交换可见项的位置，隐藏项保留原位置。切换自动排序不会丢失上次自定义顺序。

搜索图标位于各列表添加按钮左侧，点击或按 `/` 展开搜索框；再次点击或按 Escape 关闭并清空搜索。Provider 的同步状态筛选与排序在同一行。

排序方式、自定义顺序及添加时间保存在本地 `providers.json` 顶层的 `ordering` 中，并随备份恢复；排序不改写 Pi 配置或默认模型。新建、复制和明确导入的新增项记录添加时间，编辑与重命名保留原时间。历史本地配置、从 Pi 自动读取的项目及旧备份没有可靠时间时显示「添加时间未知」，在两个时间排序方向中均排在末尾。

会话选择与 URL 同步，支持深链接和浏览器前进／返回。重新扫描保留有效的当前消息、折叠状态和滚动位置；读取失败时显示错误并保留上次成功的内容。Tree 与完整阅读视图共用分支折叠和消息选择，切换模式时定位当前消息；阅读中的复制、折叠按钮与 Markdown 链接可独立使用键盘操作。

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
