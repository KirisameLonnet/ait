# ADR-029：OpenCode 原生 Session 插件与后续 Harness 扩展边界

- 状态：Superseded，2026-09-30；旧运行时由 ADR-059 退役，OpenCode 当前决策见 ADR-067，Claude Code 当前接入见 ADR-050/052。
- 范围：现有 AIT daemon/worker/Desktop。
- 上位约束：ADR-001 v4、ADR-013、ADR-017、ADR-021、NEC-169。

## 参考实现

实际对照 AIT 的 `CodexThreadWriter`、`runs/native`、`ipc/codex` 和 worker 创建、读取、
发送、关闭路径，以及 Paseo 提交 `5599f9e567128a1240b3b15afab28bceef9d36a5` 的以下文件：

- `packages/server/src/server/agent/providers/opencode/runtime-client.ts`：按原生版本选择协议。
- `opencode/server-manager.ts`、`opencode-agent.ts`：1.x 的 HTTP 服务、SSE 就绪、异步接纳和历史。
- `opencode/v2/runtime.ts`、`session.ts`、`configuration.ts`：2.x 服务认证、会话配置和模型目录。
- `opencode/v2/turns.ts`、`history.ts`、`permissions.ts`：持久执行日志、完整分页历史和审批对账。

Paseo 使用 SDK `1.14.46` 和 client `2.0.10`。本实现用 Rust HTTP 客户端实现相同原生边界，
没有引入 TypeScript SDK。协议夹具以这两个版本为基准。
参考源码：[Paseo OpenCode](https://github.com/getpaseo/paseo/tree/5599f9e567128a1240b3b15afab28bceef9d36a5/packages/server/src/server/agent/providers/opencode)。

## 决策

### 插件与进程所有权

`ait-ports` 增加中立的 `NativeSessionWriter` / `NativeSessionConnection`。打开只准备原生
Session；Start 提交一次新输入；Read 只读取；Close 停止并回收运行时。Adapter 不读写 AIT
数据库、不创建 AIT Message、不移动 Session。Codex 继续使用已经验证的专用 port 和 schema。
原生协议解析分别归属各个 adapter。

OpenCode 注册为 host-auth Provider，通过 `daemon → WorkerSupervisor → ait-worker → opencode serve`
执行。每个连接启动专属服务，使用随机认证密码、显式 loopback 地址和随机端口；不复用已有
TUI/远程服务，不接受 URL 或 provider secret。HTTP 禁用代理和重定向，输出有大小和时间上限。
worker 注册 Project 执行所有权并管理整个进程组。原生主调用沿用 Codex 的无限运行时长规则；
取消和失联仍受 worker 回收机制约束，模型发现作为有限时的辅助操作运行。

worker 增加必需的 `native-session-v1` capability、独立 Native 操作与有序分块消息，复用
已有 lease、审批、进度和收尾机制。`Start/Read/Close` 是中立控制动作，不是 Codex RPC。

### 输入与终态

application 在准备完成后原子保存 Session binding、queued Run 和 input intent。通知 worker
发送前，先持久化 `send_unknown`。原生响应丢失后不重发；恢复只使用原生历史对账。尚未发送的
queued 输入明确拒绝，避免启动恢复时产生额外外部作用。

1.x 等待 `server.connected`，使用 `prompt_async` 并观察 busy/retry/idle 和完整历史；接纳
响应不等于完成。2.x 使用 `session.active` 和有限的 durable execution log，确认最新执行已
终结后读取全部分页历史，再复核执行日志和 idle。旧的终态、started、shutdown interruption、
未完成 assistant/tool 和变动中的执行都不能被当作完成。SSE 丢失先对账；无法证明结果时保持
Interrupted/待同步，而不伪造 provider Failed。取消、超限和 panic 都关闭 worker 并条件式释放 Session。

OpenCode 1.x 的输入 ID 按当前 Paseo 源码生成原生兼容 `msg_` ID，2.x 使用 native message
metadata 关联 AIT input intent。这些标识用于对账，均不是幂等保证，也不是 AIT Message ID。

### 不可变历史与领域边界

领域新增 `SessionSource::NativeSession`，只存 driver、Provider binding、native session ID 和
同步状态。它使用已有 Session 固定 worktree，不改变 domain 的运行时依赖方向。

Adapter 输出经过校验和脱敏的中立历史 DTO。application 按父 Message 与规范化内容生成稳定
AIT UUID，原生 ID 留在 metadata。重复读取幂等；原生修订产生不可变的新后缀，不覆盖旧节点。
不再匹配已准入 Run 基准的原生修订拒绝发布。Message 和 Session 指针在同一 CAS 事务中提交。

ToolUse 保持 assistant sub-message，按原生顺序追加的 ToolResult 保持 user Message。
已关联输入和后续消息拥有同一 Run 与连续 run_seq。原生 user 输入使用 Provider provenance：
AIT 未在它的原生接纳时捕获 clean Git HEAD，不能伪造 human provenance 或 git_commit。
这沿用 ADR-017 的原生输入物化职责，但采用独立历史投影器。

### 权限与能力

生产入口只支持显式 Full Access / On Request。OpenCode 的 permission rules 不是 OS
sandbox，Read Only、Workspace Write 不作模拟。Session rule 默认 deny，允许读取类工具，
shell/edit 使用 ask。审批持久化在宿主，具体命令与路径才能进入审批；无法安全呈现的请求拒绝。
即使宿主选了 Session grant，native reply 也只授权 once。撤回和终态使待审批请求过期。

新 native items、已观察到的 tokens 和输出受宿主预算约束，超限中断；缺少可靠报价时拒绝
成本 ceiling。原生输出前已经产生的文件变更不回滚，不套用 Codex 自动提交设置。

Desktop/CLI 提供 OpenCode Provider 与原生模型/variant 选择。本次只接入从 Project 根新建、
原 Session 继续和恢复。历史导入、原生分叉、steer、交互表单、后台任务、Cron 与附件未声明可用；
不支持的入口拒绝。Claude Code CLI 原型保留，但尚未注册为生产 Provider。

## 验证

使用离线 HTTP/进程夹具、真实 worker IPC 和 application 持久化测试验证协议、发送前准入、
历史不可变、工具映射、审批、取消、预算、未知输入恢复和重启恢复。
当前机器没有 OpenCode，未进行真实登录账户与模型调用。版本兼容性、测试及覆盖率数据见
[实现报告](../reports/native-harness-adapters.md)。
