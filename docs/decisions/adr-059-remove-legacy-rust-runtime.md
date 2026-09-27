# ADR-059：移除旧 Rust 运行时

- 状态：Accepted
- 日期：2026-09-28
- 关联：ADR-022、ADR-053、ADR-057
- 修订：取代 ADR-022 的旧 daemon 并存条款及 ADR-057 第 4 条保留旧 Rust 入口的决定。

## 背景

桌面与独立服务已使用 `bins/server` 和 `server-*` crate。Cargo 声明依赖图确认，
这些包的普通、开发、构建和条件依赖均不引用旧 `ait-*` 包。
`bins/worker` 是旧 daemon 通过私有 IPC 启动的辅助进程，不能脱离被移除的旧运行时保留。

## 决策

1. 删除 `bins/daemon`、`bins/cli`、`bins/worker` 及它们的源码、测试和 fixture。
2. 删除 16 个专用 crate：`agent-adapters`、`api-http`、`application`、`contracts`、
   `domain`、`ipc`、`observability`、`ports`、`providers`、`runtime`、`sandbox`、
   `scheduler`、`storage-sqlite`、`tools`、`workspace`、`workspace-local`。
3. Cargo workspace 仅包含 `bins/server` 和 `crates/server-*`，清理无用 lockfile 条目，
   保持剩余外部包的已锁定版本。删除两个旧 Provider 真机测试脚本、仅供旧 WF-11
   使用的 `.env.example`，以及仅服务旧 shell
   sandbox 测试的 CI 配置。
4. 当前服务仍由 11 个 `server-*` crate 向内依赖；`server-domain` 保持纯领域依赖。
   本次不修改其运行行为。协议中的 `daemon.*` RPC、前端 DaemonClient 和 native
   Provider 子进程属于当前 server，不属于被移除的旧 daemon/worker 实现。
5. 根 README、文档索引和当前服务说明以现存入口为准。历史 ADR、测试报告和覆盖率制品
   保留；旧运维手册和 CLI 流程明确标记为历史，不能作为现版本可执行说明。

## 后果与验证

旧 HTTP/SSE API、CLI、SQLite 存储层及 API Provider host tool loop 不再构建或维护，
相关功能不会因删除代码自动转移到新 server。现存用户数据和凭据不作删除或迁移。
旧版本如有使用需求，可从 Git 历史获取；禁止用当前 server 直接打开旧 SQLite 状态。

依赖边界测试、全 workspace 构建/lint/测试、发布脚本检查和覆盖率结果见
[清理验证报告](../reports/remove-legacy-rust-runtime.md)。
