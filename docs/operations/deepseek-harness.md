# DeepSeek Harness

安装并配置[官方 DeepSeek Harness CLI](https://github.com/deepseek-ai/deepseek-harness)，
确认 `dsh --profile acp` 可启动 ACP stdio 服务。模型路由、凭据和 profile 配置由 Harness 管理。
Ait daemon 注册 `deepseek-harness`；客户端模型选择器显示 **DeepSeek Harness**，
模型和推理等级从本机 Harness 的实际配置目录发现。

默认从 PATH 启动 `dsh`。桌面启动环境找不到它时，在启动 server/desktop 前设置：

```sh
export AIT_SERVER_DEEPSEEK_HARNESS_BIN=/absolute/path/to/dsh
```

创建示例：

```json
{
  "config": {
    "provider": "deepseek-harness",
    "cwd": "/absolute/workspace/path",
    "title": "Harness task"
  },
  "initialPrompt": "Read this repository and explain its architecture"
}
```

向 `agent.create.request` 发送此 payload。可省略模型沿用 Harness 默认值；
指定模型时使用 `provider.models.list.request` 返回的完整 `id`，不要把 opaque ID 改写成裸模型名。
`thinkingOptionId` 也应使用返回选项的 `id`；某些模型的空字符串表示 provider default。

原生工具审批显示实际的 Allow/Reject 选项。通过现有
`agent.permission.resolve.request` 回答；`agent.cancel.request` 取消当前工作。
服务重启后 `agent.resume.request` 恢复已登记 handle，保留 Ait 保存的消息和工具时间线。

目前支持文本、附件、受 native capability 约束的图片输入、图片输出、工具生命周期、
上下文用量、stdio/HTTP MCP、审批、取消和会话恢复。
暂不支持原生会话导入、其他前端历史同步、模式、steer、rewind、commands 和结构化输出约束。
模型发现会创建并关闭一个 Harness probe session；Harness 没有会话删除接口，
因此 probe 的原生持久化记录由 Harness 的保留策略管理。

实现边界见 [ADR-067](../decisions/providers/adr-067-deepseek-harness-acp.md)。
