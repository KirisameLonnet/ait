# OpenCode 原生插件适配报告

> 历史报告：此处记录的 daemon/worker/Desktop 已随完整 main 同步退役。
> 当前 OpenCode 实现与验证见 [server 迁移报告](opencode-server-migration.md)。
> 以下测试与覆盖率仅对应原分支，不代表当前 workspace；Claude Code 当前已接入生产 server。

日期：2026-09-30。范围：现有 AIT daemon/worker/Desktop；验证版本为 `583fd18` 加本报告对应的未提交修改。

## 实现与参考

本次实际阅读 AIT 的 Codex 准备/持久化/发送/关闭/恢复实现，以及 Paseo
`5599f9e567128a1240b3b15afab28bceef9d36a5` 的 OpenCode 1.x 与 2.x adapter。
[ADR-029](../decisions/adr-029-native-harness-provider-plugins.md)记录源码路径和持久边界。

| AIT Codex 的职责 | OpenCode 实现 | Paseo 对照 |
| --- | --- | --- |
| 原生 writer port | 独立 NativeSessionWriter / Connection | provider session 生命周期 |
| worker 持有 app-server | 独立、认证的 loopback `opencode serve` | server-manager / v2 runtime |
| 准备后提交 input intent | queued → send_unknown，之后只发送一次 | prompt 接纳与执行分离 |
| 权威 thread 历史 | 1.x HTTP history；2.x 全分页 history + durable log | history / turns |
| 原生身份对账 | 1.x 原生格式 msg ID；2.x metadata correlation | 当前 v1 source / v2 client metadata |
| Message 原子发布 | 独立不可变后缀投影、Run 归因与 Session CAS | AIT 领域规则，不照搬 Paseo registry |
| durable native approval | shell/edit 具体请求，native once，撤回过期 | permissions reconciliation |
| 取消和 worker drain | interrupt/abort、进度收尾、进程组回收 | owned runtime shutdown |

已贯通模型/variant 发现、CLI Provider kind、Desktop 内置 OpenCode 配置、Session 创建/继续、
输入准入、工具历史、流式进度、取消、超限、发送结果不明对账和 daemon 重启恢复。
原生进程不读写 AIT 数据库，domain 无新增运行时依赖，旧 Codex schema 和生产 port 保持专用。

## 使用与限制

安装 OpenCode、执行 `opencode auth login`，在内置 Provider 配置中加载并选择模型，再为 Agent
选择它。支持协议为 OpenCode 1.x 和 2.0.10+，模型 ID 使用 `provider/model`。
当前生产运行要求 Full Access / On Request；native tool permissions 不能代替 OS sandbox。
Shell 与具体文件变更仍需审批，模糊资源或无法安全展示的命令被拒绝，Session grant 也只发 native once。

原生历史和执行日志有 8 MiB 总量上限，SSE frame 上限 256 KiB。新增 native item、已报告 tokens 和
输出另受 worker ceiling 约束；预算是观察后的限制，不能撤销已经执行的原生工具。未知输入不重发。
从原 Session 继续会固定使用原来的 Session worktree；不支持非根历史分叉、外部历史导入、steer、
交互表单、附件、后台任务、Cron 和 OpenCode 自动 Git 提交。Claude Code 仍只有独立 CLI 原型，
尚未注册为生产 Provider。此处没有将其计为已完成接入。

机器没有 OpenCode，未验证真实账户/模型服务与实际安装发行版。协议夹具基于 Paseo SDK
1.14.46 / client 2.0.10；真实 worker 测试启动 Python HTTP 服务，校验认证并检查服务 PID 已回收。
本次没有模型费用，也没有提交 provider token、本地数据库或运行时文件。

## 开发环境

已配置根 `flake.nix`、`flake.lock` 与 `.envrc`，覆盖 Darwin/Linux 的 arm64/x86_64 dev shell。
锁定环境提供 Rust、clippy、rustfmt、llvm tools、cargo-llvm-cov、Node 24、pnpm、Python、Git，
Linux 另外提供 bubblewrap。`.direnv/` 已忽略，使用见根 README。本次运行平台是 aarch64-darwin；
`nix flake show --offline --all-systems` 已验证四个平台的 dev shell 求值；
其他平台未执行构建与测试。

## 测试执行

| 检查 | 结果 |
| --- | --- |
| `nix develop --command cargo test --workspace -- --test-threads=1` | 859 个常规测试 + 1 个 doctest 通过，5 个既有测试忽略 |
| OpenCode adapter 专项 | 18 个原生 HTTP 插件测试通过；同名筛选另运行 1 个旧 CLI 原型测试 |
| application `opencode_execution` | 7 个通过 |
| 原生历史投影 / domain binding | 2 个 / 1 个测试通过，包含于完整 workspace 回归 |
| worker `process_opencode` | 3 个真实 worker IPC / HTTP 夹具测试通过 |
| `provider_catalog`，`dev-mock-provider` feature | 6 个通过 |
| Desktop `npm test` / `npm run typecheck` | 148 个通过 / 通过 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 |
| `cargo fmt --all --check` / `git diff --check` | 通过 |
| 最终 `cargo test -p server-bin --test process -- --test-threads=1` | 23 个通过 |

专项覆盖原生版本和地址校验、准备不发送、发送一次、恢复不重放、分页循环、旧执行终态、
未完成历史、工具规范化与脱敏、审批撤回、取消、预算、缺少 assistant 的执行失败、panic 收尾、
重启恢复和 worker 关闭。完整回归完成后，只额外修正了 server checkout 的测试收尾，
该测试目标再次分别通过普通和覆盖率运行；最终 Clippy 与格式检查也再次通过。

并行测试首次遇到既有 Codex 审批等待超时，首次覆盖率运行遇到既有 Git deadline fixture
因负载在预期 side effect 之前超时，最终改为串行运行。覆盖率模式还暴露 checkout 测试的
时序假设：取消 diff 订阅不保证已经排队的事件或已准入的 blocking 读取立即消失。
测试现会消费遗留 diff 事件，并在原有 10 秒读取窗口内等待 `resource_exhausted` 对应的
后台读取收尾。没有放宽生产超时、扩大资源额度或跳过失败用例。

忽略的 5 个测试分别需要真实 Codex/DeepSeek 账号、付费请求或独立 worker 可执行文件；
完整名称和原因见下方 JSON 工件。本机没有 OpenCode，未执行真实登录模型调用。

## Test coverage

工作区行覆盖率 **80.55%（40,745 / 50,583 行）**。
没有同口径的可比基线，不报告覆盖率增减。

| 范围 | 已覆盖 / 总行数 | 行覆盖率 |
| --- | --- | --- |
| `crates/agent-adapters` | 3,725 / 5,555 | 67.06% |
| `crates/application` | 10,247 / 12,509 | 81.92% |
| `crates/domain` | 1,012 / 1,259 | 80.38% |
| `crates/ports` | 175 / 272 | 64.34% |
| `crates/contracts` | 488 / 589 | 82.85% |
| `crates/ipc` | 931 / 1,414 | 65.84% |
| `bins/worker` | 72 / 665 | 10.83% |
| `bins/daemon` | 101 / 121 | 83.47% |
| `bins/cli` | 528 / 547 | 96.53% |
| `OpenCode adapter` | 1,397 / 1,840 | 75.92% |

测量版本为 `583fd1822b2e137cecd0f92d09d54bb1493c0d3e` 加未提交修改，Rust/Python 源码及
Cargo 配置指纹为 `5ff4534f2a349428b7e1bef4d24002730c8b31c200f74a65c7e6e37ef294fa80`。
工具为 Nix 锁定的 Rust 1.98.1 / cargo-llvm-cov 0.8.7，平台 aarch64-apple-darwin。
范围为工作区默认 features；未 instrument doctests，使用工具默认源文件过滤，没有额外手工
排除文件。导出结果不包含测试、build script 或依赖源文件。其他三个 Nix 平台仅求值，未运行测试。

测量按以下命令完成；首次全量运行在 server checkout 测试处停止，后续用 `--no-clean`
保留此前 profiles，重跑修正的测试及所有剩余包，再导出整个工作区：

```sh
nix develop --command cargo llvm-cov --workspace --html -- --test-threads=1
nix develop --command cargo llvm-cov --no-clean --html -p server-bin -p server-domain -p server-ports -p server-protocol -p server-storage -p server-workspace -- --test-threads=1
nix develop --command cargo llvm-cov report --html
nix develop --command cargo llvm-cov report --json --summary-only --output-path /private/tmp/ait-opencode-coverage-summary.json
```

可审阅工件：[逐文件覆盖率 JSON](native-harness-coverage.json)，包含工作区总数、crate 汇总、
相对源文件路径/摘要/指标、命令、测试结果和忽略清单。本地可视报告为
`target/llvm-cov/html/index.html`。

OpenCode adapter 单独为 75.92%，HTTP/SSE 错误分支、启动失败/超时与部分会话配置异常仍有
缺口；后续应针对这些分支补充故障夹具。真实发行版、登录服务、模型提供商与 Linux sandbox
也尚未执行验证。Worker 的 native/stdio 行覆盖率偏低：真实 worker 的
`cleanup_worker_group` 使用 SIGKILL 回收包含自身的进程组，LLVM counters 无法正常退出落盘；
所以 IPC 集成测试通过不能算作这些源码行已有覆盖率。后续应提供独立的、可测量的 worker
测试宿主，验证错误分支并收集 counters，同时保留生产进程回收语义。
