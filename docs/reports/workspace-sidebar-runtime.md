# Workspace 侧边栏信息与运行标记修复

日期：2026-10-02  
基线：`e098e9792699a8231c56ca0a8332e86d4007796f` 加本次未提交修改。

## 结果

侧边栏原有的增删行数、PR/MR 合并状态、CI 结果和 Git hover 信息现在由 Rust server
填充，编辑文件后通过现有 workspace 订阅持续更新。Git 与 Forge 独立缓存和排队刷新，
慢远端不会阻塞本地 Git 信息或目录请求。

运行标记会尊重 workspace 目录中较新的完成状态，旧客户端 Agent 活动不能重新点亮
已完成的行；下一轮任务仍可正常显示运行。原生会话空闲时明确返回 `activeTurn: null`。
真实 WebSocket 回归验证了未打开聊天页、未订阅 timeline 时的正常完成、取消和再次运行。

实现边界见 [ADR-068](../decisions/adr-068-workspace-runtime-summaries.md)。修改仍在源码中，
本轮没有发布或替换已安装的桌面包；实际客户端需重新构建启动后生效。

## 测试执行

所有 Rust 命令使用 `CARGO_TARGET_DIR=/private/tmp/ait-release-0.0.10-target`，
平台为 macOS arm64，默认 features，离线执行。Git/Forge 使用临时仓库和受控 CLI，
Provider 使用现有离线 Codex app-server fixture，不依赖真实凭证或远端网络。

| 范围 | 命令 | 结果 |
| --- | --- | --- |
| runtime 缓存、身份失效、Forge 错误与目录投影 | `cargo test --locked --offline -p server-filesystem -p server-metadata workspace_runtime -- --nocapture` | 10 通过 |
| 合并基点、已提交/工作树/未跟踪/二进制统计 | `cargo test --locked --offline -p server-filesystem sidebar_ -- --nocapture` | 5 通过 |
| Provider 完成后的 timeline 与目录快照 | `cargo test --locked --offline -p server-provider streaming_is_durable_searchable_and_steering_stays_in_one_native_turn -- --nocapture` | 1 通过 |
| 真实服务摘要订阅与文件编辑 | `cargo test --locked --offline -p server-bin --test process binary_workspace_runtime -- --nocapture` | 1 通过 |
| 真实服务目录订阅、完成、取消及重连 | `cargo test --locked --offline -p server-bin --test process directory_sync -- --nocapture` | 2 通过 |
| 客户端侧边栏与 Agent 状态归一化/同步 | `npm exec --workspace=@ait/app -- vitest run --project unit src/hooks/sidebar-workspaces-view-model.test.ts src/runtime/directory-sync/agent-replica.test.ts src/utils/agent-snapshots.test.ts src/utils/workspace-agent-activity.test.ts` | 64 通过 |

本地监听测试首次被 sandbox 禁止 bind；授予回环测试执行权限后通过。共享工作区中的
Diff 高亮修改未被撤销，最终检查在共享工作区执行。

格式和静态检查：`cargo fmt --all --check`、
`cargo clippy --locked --offline -p server-metadata -p server-filesystem -p server-provider -p server-bin --all-targets -- -D warnings`，
以及这次修改的两个 TypeScript 文件的 ESLint 和 oxfmt 检查。
ESLint 没有错误，保留文件中原有的 4 个 `array-type` 警告。

## Test coverage

**Not measured**。当前为本地修复迭代，只运行受影响代码及直接相关行为的定向测试，
没有运行 workspace 全量测试或 coverage。测试通过数不代表覆盖率；没有可比较的本轮
覆盖率基线、covered/total 行数或 coverage artifact。

准备 Rust 提交时按仓库政策运行 `cargo llvm-cov --workspace --html`，审查新增缓存队列、
异常恢复和状态收敛分支，并附可共享报告。当前验证未覆盖 Windows、真实 gh/glab 远端、
大量真实仓库的性能和操作系统线程创建失败；Unix 的真实服务测试及受控 CLI 覆盖了
本轮修复所需的正常路径、网络失败、旧结果失效和取消行为。
