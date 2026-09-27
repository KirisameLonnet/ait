# 新目录项目首次对话修复

日期：2026-09-27。代码基线：`b733bc0eafea43c1b076ec88a64ada0ab5f6febc` 加当前工作区修改。

## 原因与修复

新项目界面调用 `createWorkspace` 时提供首条消息的 `firstAgentContext`。Rust adapter
没有宣称支持组合创建，因此 SDK 先发送目录 Workspace 创建请求，再创建 Agent 并提交
`initialPrompt`。服务端错误地将单纯的上下文参数与组合创建一起拒绝，返回
`unsupported_capability`；公共错误文案因此误报为 `Capability was not negotiated`。

目录创建现在解析类型化的上下文，在创建回执中保留原始意图，并设置已有的
`expects_initial_agent` 元数据标记。它不调用 Provider、不消费首次消息、不读取附件，
也不新增命名生成。显式标题、资源身份、幂等重放与冲突规则不变。Agent 的实际创建和
消息发送仍由后续 Agent 请求负责；worktree 和服务端组合创建的支持范围没有扩展。

没有新增依赖或改变领域/服务边界。

## 验证

- 新增真实 server 进程回归：新建目录项目、携带首条消息上下文创建 Workspace、相同键重放、
  修改上下文的冲突、Workspace 去重、后续 Agent 创建与首次消息回复。
- 修复前，该测试在 Workspace 创建处复现截图中的精确错误；修复后通过。
- 协议测试覆盖空上下文、prompt、附件和二者组合，以及错误的上下文/prompt/附件类型。
- 桌面启动测试改为调用实际 SDK 的新目录项目与 `createWorkspace({firstAgentContext, agent})`
  路径。隔离桌面实例验证首条消息、流式追加、steer 和重启后的历史全部通过，renderer 错误为 0。
  使用已有 `apps/app/dist` 提供本地静态页面，并通过 `EXPO_DEV_URL` 指向该页面；最初未提供
  页面时开发入口的默认 8081 端口拒绝连接，补上页面后通过。所有 Provider 执行使用离线 peer。
- `cargo fmt --all --check`、`cargo clippy --workspace --all-targets --offline -- -D warnings`
  通过。桌面测试脚本的 oxfmt、oxlint 和 Node 语法检查通过；`git diff --check` 通过。
  操作手册整文件 oxfmt 检查存在基线格式问题，已在 HEAD 原文副本复核，未做无关的全文重排。
- `127.0.0.1:4567` 已以当前 debug binary 重启；`/healthz`、`/readyz` 和鉴权信息接口均返回 200。
- 完整 workspace 测试：1,707 通过、0 失败、8 ignored。跳过项沿用仓库原有标记，涉及真实
  Codex/Claude/DeepSeek 调用、已安装 Claude 探测和需独立 worker 路径的测试；逐项原因在覆盖率工件中。

## Test coverage

实测 workspace 行覆盖率 **85.2664%（57,658 / 67,621）**；相关 crate：

| 范围                                        | 已覆盖 / 总行数 | 行覆盖率 |
| ------------------------------------------- | --------------- | -------- |
| `server-metadata`                           | 5,894 / 6,607   | 89.2084% |
| `server-bin`                                | 776 / 827       | 93.8331% |
| `server-metadata/src/protocol/directory.rs` | 29 / 29         | 100%     |
| `server-metadata/src/rpc/directory.rs`      | 515 / 719       | 71.6273% |

运行命令：

```sh
RUST_TEST_THREADS=1 CARGO_INCREMENTAL=0 cargo llvm-cov --workspace --html --offline --no-fail-fast -j2
CARGO_INCREMENTAL=0 cargo llvm-cov report --json --summary-only --output-path .tmp/server-4567/workspace-coverage.json
CARGO_INCREMENTAL=0 cargo llvm-cov report --show-missing-lines
```

范围：上文 revision 加当前工作区修改，macOS arm64，全 workspace，默认 features，
cargo-llvm-cov 默认源码过滤，无额外源码或测试排除；未启用 doctest instrumentation，
不包含桌面 JavaScript 覆盖率。Linux/Windows 与真实付费模型调用未运行。

对比[此前同范围工件](codex-provider-controls-coverage.json)的 85.2595%（57,655 / 67,623），
增加 0.0070 个百分点。该工件记录的原有 Rust 修改哈希与当前文件一致；它由全量运行及
Provider 定向补跑构成，因此此处是与已有测量的比较，而非本次重新执行的隔离前后基线。

可审查工件：[覆盖率数据、逐文件结果、源码哈希及跳过项](new-project-first-conversation-coverage.json)。
完整 HTML 位于 `target/llvm-cov/html/index.html`。本次行为由协议、真实 WebSocket 和桌面 SDK
回归覆盖；目录 RPC 既有的 registry I/O 失败及失败回执恢复分支仍有缺口，后续应补充故障注入测试。
