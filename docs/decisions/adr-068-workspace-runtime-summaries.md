# ADR-068：Workspace 侧边栏运行时摘要

状态：Accepted  
日期：2026-10-02

## 背景

Paseo 的 workspace 行已经支持增删行数、分支、PR/MR 合并状态和 CI 检查结果。Ait 的
目录投影一直省略 `diffStat`、`gitRuntime`、`githubRuntime`，因此现有 UI 没有可显示的数据。
直接在每次目录订阅轮询中运行 Git 或 Forge CLI，会阻塞目录读取并重复请求远端。

另一个状态问题出现在客户端仍持有旧 Agent turn 时：workspace 目录已经返回较新的
`done` 状态，侧边栏仍无条件回退到旧 Agent 的 `running`，切换 workspace 也不能清除标记。
空闲的原生会话投影还省略 `activeTurn`，没有明确传达旧 turn 已结束。

## 决策

- 在 `server-metadata` 定义消费方拥有的 `WorkspaceRuntimeSource` 只读 port，以及 Git、
  Forge 和 diff 摘要类型。目录服务只读取缓存、映射现有 DTO；domain 不依赖 Git/CLI。
- `server-filesystem::local::workspace_runtime` 实现 port，复用 `LocalCheckout` 和
  `LocalForge`。`server-bin` 在组合根安装一个共享实例，跨连接和目录 clone 合并读取。
- 按规范化 cwd 缓存最多 1024 个目录。同一列表的同一 cwd 只投影一次。Git 和 Forge
  分别缓存 2 秒和 30 秒，各最多两个后台 worker，按队列顺序读取；不阻塞 250 ms 的目录
  订阅轮询。超过 5 秒无人请求的排队任务跳过；没有读者时不启动周期性刷新。
- Git 使用比较基点到工作树的 `--numstat -z`，包含已提交、暂存和未暂存改动。无比较
  分支时回退到 HEAD；unborn checkout 也支持。未跟踪文件按有界流读取计算文本行数，
  共享既有 4 MiB 预算，使用 `memchr` 的 SIMD 计数；二进制不计文本行数。
- 复用现有 Forge 平台识别、CLI 登录、超时和错误分类。分支、remote 或 HEAD 改变时
  清除旧 PR/MR，并拒绝旧身份的在途查询结果；临时网络失败保留同一身份的成功结果。
  无 CLI、未登录或无 remote 时保留本地 Git 信息，不虚构 change request。
- 快照、创建/打开响应与 `workspace.update` 使用现有协议和目录 sequence。明确返回
  nullable runtime 字段，让不存在/被删除的 Git 信息能够清除客户端旧值；不把刷新时间
  写入目录比较值，避免无事实变化时产生重复事件。
- 原生会话空闲时明确投影 `activeTurn: null`。客户端回退 Agent activity 时检查进入时间：
  已有同一时刻或更新的 workspace `done` 状态就结束旧标记；新 Agent 活动仍可覆盖前一轮
  的完成状态。缺少时间的旧协议维持原有回退行为。

## 后果与验证

没有新增凭证存储、目录写入或 Git 修改操作。冷缓存先返回 workspace 元数据，再由现有
订阅推送 runtime 摘要；Forge 信息可能滞后一个缓存周期。升级 server 可补齐摘要与空闲
turn 投影，旧 activity 的客户端收敛修复需要同步构建客户端。

验证范围、运行命令及 Test coverage 见[实施报告](../reports/workspace-sidebar-runtime.md)。
