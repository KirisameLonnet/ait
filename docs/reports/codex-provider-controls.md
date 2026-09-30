# Codex provider 菜单与图标对齐

## 修复

Ait 的 Rust Codex adapter 将内部支持的 `read-only` 加入了界面模式目录，
并遗漏了模式与功能按钮的图标元数据。前端因而把所有权限模式显示为
`ShieldCheck`，把 Fast、Plan 都显示为通用的 `Settings2`。

本次对照导入版本 Paseo 的 `provider-manifest.ts`、
`codex-app-server-agent.ts` 和 `codex-feature-definitions.ts`，修正 adapter 输出：

| 控件                | 模式 / 功能 ID | 图标          |
| ------------------- | -------------- | ------------- |
| Default permissions | `auto`         | `Shield`      |
| Auto-review         | `auto-review`  | `ShieldCheck` |
| Full access         | `full-access`  | `ShieldOff`   |
| Fast                | `fast_mode`    | `zap`         |
| Plan                | `plan_mode`    | `list-todo`   |

权限菜单按表中前三行排序；不支持 Auto-review 的 Codex 版本只显示
Default permissions、Full access。Fast、Plan 的标题与 tooltip 同时对齐原版。
Full access 补齐 `dangerous` 色阶与 `isUnattended` 标记。

修改仅涉及 provider 的展示目录。已有只读会话、内部只读操作与缺省配置的
原生 sandbox 映射保持原样；未迁移持久化权限，也未改变模型的快速模式能力判断。
本次没有领域边界变更。

## 验证范围

- 通过离线 Codex stdio fixture 验证 discovery 返回的选项顺序、图标与 tooltip。
- 覆盖支持和不支持 Auto-review / Plan 的两种能力发现结果。
- 同一回归继续验证 Plan 切换回普通协作模式、审批 reviewer 与原生参数。
- 既有 Fast 回归验证 `serviceTier=fast` 与不支持快速模式时的拒绝行为。
- 前端图标注册表已支持这些图标名称，本次未修改前端组件。

## Test coverage

验证基于 `b733bc0eafea43c1b076ec88a64ada0ab5f6febc` 加当前工作区改动。
工作区包含此前已有的其他修改；本报告的实现范围是五个 Codex adapter / 测试文件。

格式与 lint 已通过：

```sh
cargo fmt --all --check
cargo clippy -p server-provider --all-targets -- -D warnings
git diff --check
cargo build -p server-bin --bin server --offline
```

全工作区测试与覆盖率命令：

```sh
cargo llvm-cov --workspace --html --offline --no-fail-fast -- --test-threads=1
```

测量范围为 macOS arm64、默认 features、cargo-llvm-cov 默认源码过滤；
不包含覆盖率 doctest、其他操作系统或桌面 UI 渲染覆盖率。
本轮没有同一工作区修改前的可比覆盖率基线。

全量首轮为 **1,703 passed / 1 failed / 8 ignored**；唯一失败是原有创建会话测试
仍将 `read-only` 计入模式数量，断言预期 3 项而实际为 2 项。更新为实际模式 ID、图标
及 Fast 图标断言后，完整 `server-provider` 回归为 **370 passed / 0 failed / 3 ignored**。
为满足 Clippy 函数长度限制精简断言后，该用例再次通过（1 passed / 0 failed）。
按每个用例的最终结果汇总为 **1,704 passed / 0 failed / 8 ignored**；未将复跑次数累加。

补测沿用本轮覆盖率数据：

```sh
cargo llvm-cov -p server-provider --lib --html --offline --no-clean -- --test-threads=1
cargo llvm-cov -p server-provider --lib --html --offline --no-clean -- controls_validate_atomically_apply_next_turn_and_persist --test-threads=1
cargo llvm-cov report --html
cargo llvm-cov report --json --summary-only --output-path /tmp/ait-codex-provider-controls-coverage.json
```

| 范围                 | 覆盖行 / 总行   | 行覆盖率 |
| -------------------- | --------------- | -------- |
| Rust workspace       | 57,655 / 67,623 | 85.26%   |
| `server-provider`    | 14,414 / 15,557 | 92.65%   |
| Codex `controls.rs`  | 235 / 240       | 97.92%   |
| Codex `workflows.rs` | 133 / 135       | 98.52%   |

[可审阅的覆盖率数据](codex-provider-controls-coverage.json)包含 workspace 汇总、
`server-provider` 逐文件结果、命令、测试结果和五个变更文件的 SHA-256。
本地完整 HTML 位于 `target/llvm-cov/html/index.html`。

这两个生产文件的主要未覆盖路径是原有子代理发现中的过滤 / 异常分支，以及
`collaborationMode/list` 请求被拒绝或失败的分支；后续可扩充离线 fixture 覆盖这些路径。
本次未执行真实模型请求、桌面截图验证或其他操作系统测试。
