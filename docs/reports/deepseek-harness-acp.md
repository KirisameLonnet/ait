# DeepSeek Harness ACP 验证

验证日期：2026-10-02。Revision：`e098e9792699a8231c56ca0a8332e86d4007796f` 加本 PR 源码修改。
测量源码的聚合 SHA-256 为 `4e9298bbbec815b3edb4644b7abd5a8e1a60cbc4fb743ad3b570e8d56f488fba`；
聚合算法、变更文件哈希和逐文件结果收录在[覆盖率工件](deepseek-harness-acp-coverage.json)。
平台 macOS 27.0 (26A428) arm64，Rust 1.98.1，LLVM 22.1.8，cargo-llvm-cov 0.8.4。

## 交付行为

新增 `deepseek-harness` 原生 provider，从 `dsh --profile acp` 建立 ACP v1 stdio 连接。
Rust adapter 接通模型/推理选择、创建与恢复、文本/图片/工具投影、审批、取消和上下文用量；
客户端加入 provider definition、ACP catalog 和官方图标。
`AgentClient::supports_history_replay()` 让无 transcript replay 的 provider 保留既有时间线。
架构边界见 [ADR-067](../decisions/adr-067-deepseek-harness-acp.md)，
安装和入口见[使用说明](../operations/deepseek-harness.md)。

## Test execution

在提交准备阶段执行完整 workspace 回归和覆盖率运行；两轮均为 **1499 passed、0 failed、3 ignored**。
忽略项为已有的 Claude/Codex 真实 CLI 握手或模型推理用例，需本机安装/认证。
覆盖率运行串行执行测试；普通 workspace 测试使用默认并发。

| 命令                                                                                                                                                                                    | 结果                                |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------- |
| `cargo test --workspace`                                                                                                                                                                | 1499 passed，3 ignored              |
| `cargo llvm-cov --workspace --html --no-fail-fast -- --test-threads=1`                                                                                                                  | 1499 passed，3 ignored；HTML 已生成 |
| `cargo test -p server-provider deepseek_harness`                                                                                                                                        | 14 passed                           |
| `cargo build --workspace`                                                                                                                                                               | passed，无警告                      |
| `cargo fmt --all --check`                                                                                                                                                               | passed                              |
| `cargo clippy --workspace --all-targets -- -D warnings`                                                                                                                                 | passed                              |
| `npm run build:protocol`                                                                                                                                                                | passed                              |
| `npm exec --workspace=@ait/app -- vitest run --project unit src/hooks/use-acp-provider-catalog.test.ts src/components/provider-icon-name.test.ts src/components/provider-icons.test.ts` | 19 passed                           |
| `npm exec --workspace=@ait/protocol -- vitest run src/provider-manifest.deepseek-harness.test.ts src/paseo-config-schema.test.ts`                                                       | 14 passed                           |
| `oxfmt --check` / `oxlint`（变更 TS 和 ACP fixture）                                                                                                                                    | passed                              |
| `git diff --check`                                                                                                                                                                      | passed                              |
| `npm run typecheck --workspace=@ait/app`                                                                                                                                                | failed：63 项诊断，与基线完全相同   |

App 类型检查缺少本机已安装依赖，包括 `expo-clipboard`、`react-native-keyboard-controller`、
`htmlparser2`、`@xterm/*`，并存在 `node:sqlite` 类型诊断。
通过 `git archive HEAD apps/app` 创建临时基线、链接同一 `node_modules`，在基线中运行
`tsgo --noEmit`，得到完全相同的 63 项错误；本次变更文件没有诊断。
完整安装依赖后的 App 类型检查交由 CI 验证，未将本机失败记为通过。

离线 ACP peer 验证能力声明、grouped opaque model ID、模型切换后的推理选项、
空值 provider default、message/thought chunk 合并、工具快照合并、图片物化、
大输出预览、真实权限 option ID、前端 Deny payload、过期/行为不匹配响应、
取消、恢复、临时环境不进入 handle、foreign session 拒绝以及坏版本/JSON/超大帧/超时。

真实 Rust server 进程通过 WebSocket 完成 provider discovery、创建、发送、审批与 finish-wait，
服务重启后比较完整 timeline entries 和 epoch，再恢复同一 handle 并取消下一轮。
进程测试需 loopback 端口，已通过批准的测试执行权限运行。

## Test coverage

| 范围                          | 覆盖行 / 总行 |   行覆盖率 |
| ----------------------------- | ------------: | ---------: |
| Cargo workspace               | 42648 / 46363 | **91.99%** |
| `server-provider`             | 18340 / 19640 | **93.38%** |
| `server-bin`                  |     876 / 929 | **94.29%** |
| DeepSeek Harness 生产 adapter |    960 / 1020 | **94.12%** |

测量覆盖完整 Cargo workspace、默认 features，使用 cargo-llvm-cov 默认源文件过滤，
没有额外排除。未对 doctest、TypeScript/UI 做覆盖率插桩；默认忽略的 3 项真实 CLI 用例未执行。
没有相同源码修改前的可比较覆盖率基线，因此不报告增量百分比。

```sh
cargo llvm-cov --workspace --html --no-fail-fast -- --test-threads=1
cargo llvm-cov report --json --summary-only --output-path /private/tmp/ait-deepseek-coverage-summary.json
cargo llvm-cov report --lcov --output-path /private/tmp/ait-deepseek-coverage.lcov
```

可共享审查工件：[覆盖率与测试结果 JSON](deepseek-harness-acp-coverage.json)，
包含 workspace/各 crate 计数、adapter 逐文件指标、未覆盖行、源码哈希、命令和测量范围。
完整本机 HTML 为 `target/llvm-cov/html/index.html`；已结合 LCOV 审查 adapter 未覆盖行。

主要未覆盖行为为不支持 resume/close 的 native capability 变体、prompt error/未知 stopReason、
未知 callback 拒绝、队列上限，以及 stdin 写入失败/超时等异常分支。
扩大 ACP 能力范围前应补充 malformed peer 和 backpressure 场景。
未执行真实 Harness profile/认证、模型推理和 native 工具；需在具备 CLI/profile/认证的环境验证。
Linux 和 Windows 未在本机实测；Linux suite 由 PR CI 执行，Windows 进程清理需对应宿主验证。
