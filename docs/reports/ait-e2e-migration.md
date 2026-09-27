# Ait E2E、运行路径与旧功能清理

日期：2026-09-28。基线：`0d42467` 加本次未提交工作区改动。

## 结果

- App E2E 只启动隔离的 Rust `server`，使用独立数据目录、端口、Bearer token、真实 server UUID、生产 Rust transport 和浏览器认证票据。
- 默认 Provider 使用 Rust 测试里的离线 Codex stdio fixture；真实 Provider 需显式启用。日志写入隔离目录的 `server.log`。
- App 移除 relay 连接、扫码/配对、插件运行时、路由、UI 扩展与直接依赖；旧持久化 relay 连接和插件面板在恢复时丢弃。
- 移除不再发行的 Node CLI 安装入口、shim、passthrough，以及依赖旧 Node server/插件/relay 的桌面测试启动器。现有 Ait GUI 和 Rust 生命周期测试保留。
- 工作区归属使用 server 元数据；项目配置默认 `ait.json`，兼容读取旧文件，保存不覆盖旧文件。原生附件缓存、搜索目录与克隆临时目录采用 Ait 名称。
- 项目版权、作者和维护者更新为 Necokeine；上游及第三方许可归属保留。真实仓库目录、共享 SDK 包名、IPC 与持久化存储键未统一改名。

## 验证

仅运行受影响代码的定向检查，未运行全量测试。

- `cargo build -p server-bin --bin server --target-dir target`：通过。
- `cargo test -p server-metadata --lib project_config`：7 项通过。
- `cargo test -p server-metadata --lib local::workspace_automation`：17 项通过。
- `cargo test -p server-filesystem --lib local::worktrees`：34 项通过。
- `cargo test -p server-filesystem --lib local::files::tests::search_filters`：1 项通过。
- `cargo test -p server-filesystem --lib local::github_projects`：14 项通过。
- `cargo clippy -p server-filesystem -p server-metadata --all-targets -- -D warnings`、最终受影响文件系统 crate 的增量检查和 `cargo fmt --all --check`：通过。
- `npm run typecheck --workspace=@getpaseo/app`、`npm run typecheck --workspace=@getpaseo/desktop`：通过。
- 修改的 TypeScript 文件：`oxfmt` 与 `oxlint -A no-empty-pattern` 通过；该 lint 例外用于 Playwright fixture 的空解构参数。
- App 路径/归属/路由/文件链接/工作区配置：7 个文件 117 项单测通过；连接探测单独重跑 8 项通过；面板 manifest / launcher 4 项通过。此前插件/relay 删除涉及的存储、运行时和 UI 模型定向测试也已通过。
- SDK `daemon-client-transport`：10 项通过；连接、认证与重连筛选回归：14 项通过，124 项未选中。`npm run typecheck --workspace=@getpaseo/client` 通过。
- 桌面 `desktop-startup`、`desktop-packaging`、`daemon-manager`：3 个文件 11 项通过。
- `E2E_AIT_SERVER_BIN="$PWD/target/debug/server" npm run test:e2e --workspace=@getpaseo/app -- e2e/browser/ait-server.spec.ts e2e/browser/workspace-model-restart.spec.ts e2e/browser/daemon-lifecycle.spec.ts`：6 项通过，覆盖认证、票据连接、真实 Rust Provider 离线对话、配置迁移、进程内服务重启、进程重启后工作区恢复。SDK relay 静态依赖移除后最终复跑仍为 6 项通过。
- 此前主题选择、空会话、Host 设置页面的定向浏览器回归通过。
- 测试收集通过：App browser 596 项 / 165 文件，desktop renderer 19 项 / 8 文件。收集成功不代表全量执行通过。

需要临时本地监听端口的测试已在允许绑定端口的环境运行；沙箱内首次执行的端口权限错误不属于产品失败。原生移动端构建和真实在线模型 Provider 未运行。

## Test coverage

**未测量**本次行覆盖率，没有可比基线或新的覆盖率产物。本地迭代仅执行受影响模块的定向测试；按仓库约定，工作区覆盖率延后至提交准备阶段，用 `cargo llvm-cov --workspace --html` 生成并分享报告。通过的测试数量不代表覆盖率。

## 已知范围

继承的 browser 用例仍包含旧 mock Provider、目录订阅、创建回执和旧 wire-frame 断言。本次将所有保留的启动器切换到 Ait，并建立上述可执行回归；没有声明这些继承场景已全部适配。共享 SDK 的兼容类型和包仍存在。

运行入口见 [App E2E](../../apps/app/e2e/README.md)，决策见 [ADR-061](../decisions/adr-061-app-ait-e2e-remove-relay-plugin.md) 与 [ADR-062](../decisions/adr-062-ait-runtime-paths.md)。

共享 client 已移除 relay/E2EE transport、静态导出和 `@getpaseo/relay` 依赖，并拒绝旧 relay URL。App 的 Metro relay 解析特例、扫码依赖和权限也已移除；独立历史包与兼容 wire 类型不属于 App 运行依赖链。
