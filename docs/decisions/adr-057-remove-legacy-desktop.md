# ADR-057：移除旧版桌面实现

- 状态：Accepted
- 日期：2026-09-27
- 关联：ADR-053
- 后续：第 4 条保留旧 Rust 入口的决定已由 [ADR-059](adr-059-remove-legacy-rust-runtime.md) 取代。
- 修订：取代 ADR-053 中保留旧桌面源码与独立版本的条款。

## 背景

正式桌面构建、CI 和发布已使用 `apps/paseo`、`apps/app` 与独立 Rust `server`。
`apps/desktop` 是不再发布的旧 Electron 实现，仍保留独立依赖、构建脚本和测试，
部分操作文档与验证脚本还引用其目录。

## 决策

1. 删除 `apps/desktop`，包括源码、测试、独立 manifest/lockfile、打包配置和本地构建产物。
2. 桌面开发与发布继续通过根 npm workspace 使用 `apps/paseo` 和 `apps/app`。
   验证脚本从根 workspace 或显式指定的测试依赖目录解析依赖。
3. 清理旧目录的忽略规则、操作命令和当前文档说明；历史 ADR、报告与覆盖率制品保留作追溯。
4. 保留旧 Rust daemon、worker、CLI 及其契约与测试；本次不改变 Cargo workspace、
   领域模型或依赖方向，也不迁移或删除用户数据。

## 验证与后果

旧桌面的独立构建和 GUI 测试入口随目录移除。当前桌面继续使用既有构建与发布流程。
验证脚本语法、格式与 lint，检查依赖解析、发布版本及当前配置中是否仍引用旧目录。
本次没有修改 `bins/` 或 `crates/` 下的 Rust 代码，按仓库约定跳过 Rust 测试。
