# Ait 0.0.7 发布准备

基线：`80e97c0` 加当前工作区；本报告描述发布准备，不表示已经创建标签或发布 GitHub Release。
决策见 [ADR-053](../decisions/adr-053-paseo-desktop-release.md)。

## 已完成

- 活跃 Cargo/npm workspace 版本和锁文件统一为 0.0.7；更新应用内 CHANGELOG。
- Release 与桌面 CI 切换到 `apps/paseo`，前端导出来自 `apps/app`。
- 平台保持 Linux x86_64 与 macOS arm64；Windows、deb/rpm 不进入发布目标。
- 构建范围固定为 `server-bin --bin server`。暂存验证版本并清理旧 sidecar；安装包的
  `resources/bin/` 必须只有一个可执行文件 `server`，ASAR 不能包含 Node server/CLI 包。
- 正式应用 ID 采用 `dev.ait.desktop`；macOS 为 server 签名并要求公证。
- 安装包、更新 YAML、blockmap 一同收集，检查 SHA-512/版本/引用，再生成覆盖全部资产的 SHA256SUMS。
- 成品冒烟测试验证自动启动、鉴权 RPC、服务重启与重连、应用退出后的进程回收。

## Test coverage

Rust 行覆盖率：**不适用，本次未修改 Rust 源代码或运行行为**；只更新 Cargo 版本约束和锁文件。
按 AGENTS.md 跳过 Rust workspace tests 与 llvm-cov。没有把测试通过数量当作覆盖率。
JavaScript/TypeScript 行覆盖率未测量；本轮验证范围是发布故障门禁、桌面回归与成品运行。

| 验证                                                                                                                          | 结果                                                                                        |
| ----------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| `npm run verify:release -- v0.0.7`                                                                                            | 9 个活跃 npm 包、lockfile、Cargo workspace 和标签一致                                       |
| `npm install --package-lock-only --ignore-scripts --offline --no-audit --no-fund`                                             | npm 锁文件解析通过，无依赖升级                                                              |
| `npm run test:release`                                                                                                        | 9 项通过；覆盖旧二进制残留、错版 server、执行权限、缺失资产、额外平台资产、更新摘要和校验和 |
| `AIT_SERVER_BIN=.../target/release/server npm test --workspace=@getpaseo/desktop`                                             | 389 项通过，9 项按平台或环境跳过；真实 Rust 子进程测试启用                                  |
| `npm run test --workspace=@getpaseo/app -- src/i18n/resources.test.ts src/runtime/rust-server native-release-version.test.ts` | 70 项通过                                                                                   |
| `npm run typecheck --workspace=@getpaseo/desktop --workspace=@getpaseo/app`                                                   | 通过                                                                                        |
| 变更脚本的 oxfmt、oxlint 与 `git diff --check`                                                                                | 通过                                                                                        |
| `cargo metadata --offline --format-version 1`                                                                                 | 完整 workspace 解析通过并更新锁文件                                                         |
| `cargo build --locked --offline --release -p server-bin --bin server`                                                         | 通过，产物报告 `server 0.0.7`                                                               |
| `cargo check --workspace --locked --offline`                                                                                  | 通过，完整 Cargo workspace 保持可构建                                                       |
| Electron Web 导出                                                                                                             | 通过，确认嵌入 0.0.7 发布说明                                                               |
| 本地 macOS DMG 与成品 server 生命周期                                                                                         | 通过，renderer error 为 0                                                                   |
| 正式文件布局的本地 macOS DMG/ZIP/更新元数据                                                                                   | 通过；实际 latest-mac.yml 的引用、大小、SHA-512 校验通过                                    |
| ZIP 内容与 DMG 完整性                                                                                                         | ZIP 内只有 `Contents/Resources/bin/server`，Bundle ID/版本正确；`hdiutil verify` 通过       |

本机为 macOS arm64。运行时验证使用临时 server 和 Electron 数据目录，结束后清理；没有
执行用户已有的定时任务或修改用户项目。最初受沙箱端口限制的生命周期测试已在允许本机
监听的环境中重跑通过。发布工具的 Node tests 与桌面 Vitest 已分开，避免互相误识别。

本地验证日志在 `.tmp/release-*.log`；DMG、ZIP 和更新资产在 `.tmp/release-validation/mac/`。
这些是临时验证产物，没有上传。正式布局的本地打包命令在生产配置基础上显式覆盖了
`mac.identity=-`、`mac.hardenedRuntime=false`、`mac.notarize=false`，只验证布局和运行能力，
不能代替 Developer ID 签名、公证或正式分发。

## 本地 Developer ID 签名验证

在上述布局验证之后，使用本机 `Developer ID Application: Dong Shan (SVS7GV79T9)`
生成 `apps/paseo/release/signed/Ait-0.0.7-macos-arm64.dmg`。应用和内置 server 启用了
hardened runtime，DMG、应用及 server 均有 Developer ID 签名和时间戳。

- `codesign --verify --deep --strict` 校验应用通过；server 和 DMG 的独立签名校验通过。
- `hdiutil verify` 通过；只读挂载后再次确认应用版本、Bundle ID、签名及 `bin/server` 唯一性。
- 正式签名后的成品启动、鉴权 RPC、server 重启重连和退出回收验证通过，renderer error 为 0。
- 本机未配置公证凭据，本次产物未提交 Apple 公证，也未上传 GitHub。
- GitHub 仓库的五项发布 Secrets 名称已核验存在；凭据有效性仍需由正式发布流程验证。

签名产物和验证明细保留在被忽略的本地输出目录，不纳入源码提交。

## 发布前仍由 CI 验证的范围

- Linux 原生编译、AppImage/tar.gz 生成与 Xvfb 成品启动测试；本机没有运行 Linux 二进制。
- 使用 GitHub Secrets 完成 macOS Developer ID 签名和 Apple 公证。
- 双平台资产汇总与实际 GitHub Release 上传；本次没有创建、移动或推送标签。

工作流使用 `ubuntu-24.04` 和 `macos-15`；后者的 arm64 架构已按
[GitHub 官方 runner 文档](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)核对。

旧桌面数据库不自动迁移到 `~/.ait-server-desktop`，旧文件保留；这项边界已写入 ADR 和
[发布操作指南](../operations/releasing.md)。
