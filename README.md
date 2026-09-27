# Ait

<img src="logo.svg" alt="Ait logo" width="96" height="96" />

Ait 是一个本地优先的多 Agent 管理器，目标是统一在线协作平台、本地 Agent 运行时和面向任务的管理界面。

桌面应用由 `apps/paseo` 承载，界面位于 `apps/app`，内置服务实现语言固定为 Rust。核心概念与边界以 `docs/README.md` 中列出的 ADR 为准。

## 开始开发

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

npm ci
npm run dev:desktop
```

## 本地 server

```bash
cargo run -p server-bin --bin server -- --help
```

正式桌面安装包只携带独立 `server`，详见 [server 说明](docs/operations/independent-server.md)。

## Workspace

- `bins/server`：桌面和独立运行的 Rust 服务入口。
- `crates/server-domain`：不依赖运行时或传输层的领域记录。
- `crates/server-model`、`crates/server-protocol`：公共请求上下文、错误模型与 WebSocket 协议。
- `crates/server-api`：HTTP/WebSocket 接入、鉴权与能力分发。
- `crates/server-metadata`：Project/Workspace 目录、配置与元数据。
- `crates/server-filesystem`：文件、Git、worktree 和 Forge 操作。
- `crates/server-provider`：Codex/Claude 原生会话、历史投影与 metadata generation。
- `crates/server-terminal`、`crates/server-voice`、`crates/server-schedule`、`crates/server-browser`：终端、语音、调度和浏览器能力。
- `packages/`：本地私有 `@ait/client`、`@ait/protocol`、`@ait/highlight` 与 `@ait/expo-two-way-audio` 源码；内部依赖使用 `file:`，通过 `npm run verify:local-packages` 校验。
- `apps/paseo`：Electron 桌面与 Rust server 生命周期管理。
- `apps/app`：桌面、Web 与移动端共享界面。

旧 daemon、worker、CLI 及其专用 crate 已移除，见
[ADR-059](docs/decisions/adr-059-remove-legacy-rust-runtime.md)。旧版操作文档仅供历史追溯；
当前入口和协议见 [server 说明](docs/operations/independent-server.md)。

GitHub Release 为 Linux x86_64 与 Apple Silicon 构建 **Ait** 桌面产物；
版本准备、产物校验和故障恢复见 [发布操作指南](docs/operations/releasing.md)。
