# 移除旧 Rust 运行时

日期：2026-09-28。边界决定：[ADR-059](../decisions/adr-059-remove-legacy-rust-runtime.md)。

## 清理范围

删除 `bins/daemon`、`bins/cli` 及只供旧 daemon 使用的 `bins/worker`，连同 16 个专用 crate：
`agent-adapters`、`api-http`、`application`、`contracts`、`domain`、`ipc`、`observability`、
`ports`、`providers`、`runtime`、`sandbox`、`scheduler`、`storage-sqlite`、`tools`、
`workspace`、`workspace-local`。源码、测试、fixture、两份旧真机测试启动脚本及 WF-11 的 `.env.example` 共删除 314 个文件。

Cargo workspace 从 31 个 package 收敛为 `server-bin` 和 11 个 `server-*` crate；
保留的生产和测试源码没有修改。普通、开发、构建和条件依赖的内部 path 均解析到保留包。
`Cargo.lock` 从 488 个包降为 265 个，删除 223 个条目，未引入新版本或升级现存包。
CI 删除旧 shell sandbox 专用的 bubblewrap 安装及 `AIT_REQUIRE_SHELL_SANDBOX`。

根 README、文档索引、当前 server 操作说明更新为现存实现；旧运维与 CLI 流程标记为历史。
历史 ADR、报告、覆盖率制品仍保留；旧流程的源码链接固定到删除前的提交，避免悬空链接。当前 `daemon.*` RPC、前端 DaemonClient、Provider
原生子进程，以及打包测试中“拒绝旧二进制”的反例，均属于当前 server 的有效代码。
旧 SQLite 数据、凭据及运行目录未删除或迁移；旧 HTTP/SSE、CLI 和 host tool loop 随源码退役，
本次没有把它们新增到当前 server。

## 验证

测量基线：`49478a7f600fde997339a8d36d368c72d3546c14` 加本次清理。
普通 Cargo 检查使用 `CARGO_TARGET_DIR=/private/tmp/ait-agent-interface-target`，默认 features：

- `cargo test --offline -p server-bin --test dependencies`：10 passed，0 failed。
- `cargo build --workspace --locked`：通过，无编译警告。
- `cargo clippy --workspace --all-targets --locked -- -D warnings`：通过。
- `cargo test --workspace --locked`：1260 passed，0 failed，3 ignored。
- `cargo fmt --all --check`、`git diff --check`：通过；所有改动的 Markdown/YAML 通过 `oxfmt --check`。
- `cargo metadata --locked --no-deps --format-version 1`：12 个 server 包，无旧包及悬空 path。
- `node scripts/verify-release-version.mjs`：通过。
- `npm run test:release`：12 passed，0 failed；CI YAML 解析通过。

3 项忽略项均为已有的真实 Codex/Claude 安装或鉴权测试；未新增跳过测试。
发布测试首次因当前 worktree 未安装 Node 依赖而未能加载，随后复用主仓库依赖，先核对
`builder-util`、`app-builder-lib`、`@electron/asar`、`yaml` 与锁文件版本一致，再完整运行通过；
临时 `node_modules` 链接已经移除，没有修改 npm manifest 或锁文件。
当前 server 的应用源码与界面未变化，未进行桌面安装包或 UI 手工验收。

## Test coverage

当前完整 workspace 行覆盖率 **91.56%（35,498 / 38,771）**。
从[上一版制品](server-metadata-generation-coverage.json)筛选同样的 `server-bin` 与 11 个
`server-*` crate，基线为 **91.55%（35,495 / 38,771）**，变化为 **+0.0077 个百分点**。
逐文件 SHA-256 核对确认保留的源码完全一致。本次没有新增测试；移除旧包前的整仓
85.52% 包含其他实现，不能直接用来比较本次测试质量。

| 范围                     | 已覆盖 / 总行数 | 行覆盖率 |
| ------------------------ | --------------- | -------- |
| bins/server              | 834 / 883       | 94.45%   |
| crates/server-api        | 1,104 / 1,149   | 96.08%   |
| crates/server-browser    | 707 / 726       | 97.38%   |
| crates/server-domain     | 121 / 121       | 100.00%  |
| crates/server-filesystem | 7,759 / 8,821   | 87.96%   |
| crates/server-metadata   | 6,061 / 6,758   | 89.69%   |
| crates/server-model      | 296 / 310       | 95.48%   |
| crates/server-protocol   | 57 / 57         | 100.00%  |
| crates/server-provider   | 15,250 / 16,415 | 92.90%   |
| crates/server-schedule   | 802 / 820       | 97.80%   |
| crates/server-terminal   | 1,289 / 1,436   | 89.76%   |
| crates/server-voice      | 1,218 / 1,275   | 95.53%   |

测量为 macOS aarch64、默认 features、整个剩余 workspace，无显式文件排除；
测试源码由 cargo-llvm-cov 默认规则排除，doctest 未插桩（本 workspace 没有 doctest 用例）。
3 项真实 Provider 测试仍默认忽略。Linux/Windows 未在本机测量覆盖率。
测量 revision 为 `49478a7f600fde997339a8d36d368c72d3546c14` 加本次清理，
[共享 JSON 制品](remove-legacy-rust-runtime-coverage.json) 包含完整行数、crate 汇总、
逐文件源码哈希、对比基线及测试结果；本地 HTML 位于
`/private/tmp/ait-session-titles-coverage/llvm-cov/html/index.html`。

最终精确命令：

```sh
CARGO_TARGET_DIR=/private/tmp/ait-session-titles-coverage cargo llvm-cov clean --profraw-only
CARGO_TARGET_DIR=/private/tmp/ait-session-titles-coverage cargo llvm-cov --no-clean --workspace --locked --html -- --test-threads=1
CARGO_TARGET_DIR=/private/tmp/ait-session-titles-coverage cargo llvm-cov report --json --summary-only --output-path /private/tmp/ait-remove-legacy-workspace-coverage.json
```

最终覆盖率运行 **1260 passed，0 failed，3 ignored**，与普通全量测试结果一致。
清空旧 profile 后使用 `--no-clean` 只复用编译产物，不混入前几轮测试的覆盖率数据。
此前 4 线程插桩运行中 8 项原生创建返回 `AgentIo`，第一次单线程运行有 2 项 WebSocket
等待超时；单线程定向控制测试 7 项通过，最终完整重跑全部通过。期间观察到较高系统负载，
尚未确定这些间歇失败的根因；没有修改生产超时、测试代码或通过跳过用例规避失败。

未覆盖路径仍主要为部分原生异常帧、registry/文件 I/O 故障与取消竞争；真实付费模型、
Windows 原生进程清理及桌面 UI 需要对应环境验证。原生测试的插桩时序稳定性仍需后续观察。
