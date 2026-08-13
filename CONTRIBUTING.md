# 贡献指南

感谢愿意给 [pi-switch](https://github.com/OldSuns/pi-switch) 提改动。产品说明见 [README](./README.md)；这里只写怎么在本地开发和提交。

## 环境

- Node.js `>= 20`
- Rust 工具链（含 `rustfmt`、`clippy`）
- `@napi-rs/cli`（`npm install` 会装到 devDependencies）

## 本地运行

```bash
npm install
npm run build:native:debug
node ./bin/pi-switch.js
```

## 提交前

与 CI 一致，请先跑过：

```bash
cargo test --locked --lib
cargo fmt -- --check
cargo clippy --locked --all-targets -- -D warnings
```

需要的话再补：

```bash
npm run build:native:debug
npm run pack:check
```

## Pull Request

- 对着 `main`
- 保持最小 diff，只改这次要解决的问题
- commit message 沿用现有风格：`feat:` / `fix:` …
