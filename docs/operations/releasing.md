# Ait 发布操作指南

从 0.0.7 起，GitHub Release 构建 `apps/paseo` Electron 桌面、`apps/app` 的 Web 导出和
Rust `server`。发布边界见 [ADR-053](../decisions/adr-053-paseo-desktop-release.md)，
旧桌面源码的移除见 [ADR-057](../decisions/adr-057-remove-legacy-desktop.md)。

## 发布产物

| 平台     | 架构                | 文件                                                     |
| -------- | ------------------- | -------------------------------------------------------- |
| Linux    | x86_64              | `Ait-linux-x86_64.AppImage`                              |
| Linux    | x86_64              | `Ait-VERSION-linux-x64.tar.gz`                           |
| macOS    | Apple Silicon arm64 | `Ait-VERSION-macos-arm64.dmg`                            |
| macOS    | Apple Silicon arm64 | `Ait-VERSION-macos-arm64.zip`                            |
| 自动更新 | 各平台              | `latest-linux.yml`、`latest-mac.yml`、生成的 `.blockmap` |
| 校验     | 全部资产            | `SHA256SUMS`                                             |

AppImage 文件名保持稳定，版本体现在 Release 标签和应用内部。Windows、deb/rpm、其他架构
和独立 CLI 不属于本次发布。安装包 `resources/bin/` 中只有 `server`；Electron 主程序与
Helper 是必需运行时。GitHub 仍自动提供标签对应的源码归档。

## 准备版本

同步根 `Cargo.toml` 的 `workspace.package.version`、带版本约束的本地 Cargo path 依赖、
Cargo.lock、根 package.json、所有活跃 npm workspace 及其 lockfile。然后运行：

```bash
npm ci
npm run verify:release -- v0.0.7
npm run test:release
npm run build:desktop-main
npm run typecheck --workspace=@ait/desktop --workspace=@ait/app
```

更新根 CHANGELOG.md：该文件
会打入应用，供“新功能”页面读取。把版本与发布变更经 PR 合并到 `main` 后，再创建标签。

## 创建 Release

```bash
git switch main
git pull --ff-only
git tag -a v0.0.7 -m "Ait v0.0.7"
git push origin v0.0.7
```

`.github/workflows/release.yml` 在 Linux x86_64 和 macOS arm64 原生 runner 上执行：

1. 校验标签和全部活跃版本，安装根 npm workspace，验证发布脚本。
2. 用锁定依赖只构建 `server-bin` 的 `server`。
3. 导出界面、编译 Electron 主进程，验证 server 版本并暂存单个可执行文件。
4. 检查打包内容；macOS 签名、公证；隔离启动成品应用并验证真实 server 生命周期。
5. 收集两种平台的安装包和自动更新资产，核对更新摘要。
6. 两个平台都成功后生成 SHA256SUMS，再创建或修复 GitHub Release。

macOS 需要 GitHub Secrets：`MAC_CSC_LINK`、`MAC_CSC_KEY_PASSWORD`、`APPLE_ID`、
`APPLE_BUILD_APP_SECRET`、`APPLE_TEAM_ID`。缺失签名、公证凭据会阻止发布；不会降级为未签名包。
Release Note 由 `.github/release.yml` 根据合并 PR 分组生成。

手动重跑：Actions → Release Ait → Run workflow，输入已存在的标签。工作流不会创建标签。
已有 Release 保留说明，覆盖同名资产；不要移动已经公开使用的标签。

默认应用源码从输入的发布标签检出；资产收集、校验和相关测试从工作流自身的提交检出到
`.tmp/release-tools`。因此修复发布工具后，可以选择包含修复的工作流分支重跑原始标签：

```bash
gh workflow run release.yml --ref main -f tag=v0.0.7
```

修复尚在 PR 分支时，`--ref` 可以指定该分支。`github.workflow_sha` 固定该次运行使用的
工具提交，应用仍由原始标签构建，不改写标签。普通 Re-run jobs 沿用旧工作流，不能加载
新提交的工具修复。[GitHub 工作流版本说明](https://docs.github.com/en/actions/reference/workflows-and-actions/variables)

明确要求更新既有版本的安装包时，可以额外传入 `source_commit`（必须是完整 40 位 SHA）。
两个平台和发布步骤均从该提交检出，仍须通过版本、签名、公证和成品启动门禁。
这不会移动原标签；`BUILD-INFO.json` 记录源码、工作流提交和运行链接，随校验和一起上传。
必须在 Release Note 中说明重建修复、实际源码提交及关联 PR，提醒同版本用户重新下载安装。
GitHub 自动生成的 Source code 归档仍对应原标签；修复后的源码应链接到 `sourceCommit`。

```bash
gh workflow run release.yml --ref YOUR_PR_BRANCH -f tag=v0.0.7 -f source_commit=FULL_COMMIT_SHA
```

## 本地验证

Linux x86_64：

```bash
npm ci
cargo build --locked --release -p server-bin --bin server --target x86_64-unknown-linux-gnu
AIT_SERVER_BIN="$PWD/target/x86_64-unknown-linux-gnu/release/server" \
  AIT_DESKTOP_SMOKE=1 npm run package:linux
```

Linux 需安装 `xvfb`、FUSE 和 Electron 的系统库；具体包名见 workflow。

macOS arm64（正式签名、公证）：

```bash
npm ci
# 配置 CSC_NAME 或 CSC_LINK，以及 Apple 公证凭据后：
AIT_DESKTOP_SMOKE=1 npm run package:mac
```

`package:mac` 生成 DMG 与 ZIP；`package:linux` 生成 AppImage 与 tar.gz。省略 `AIT_SERVER_BIN`
会从当前源码构建 release server；提供该变量时仍检查二进制版本。只允许原生目标平台、架构。

无签名凭据时，本机开发验证使用 `AIT_DESKTOP_SMOKE=1 npm run build:dmg`。它生成
`Ait-VERSION-local-arm64.dmg`，不属于正式发布文件，不能通过正式资产收集门禁。

输出位于 `apps/paseo/release/`，暂存输入位于 `apps/paseo/release-resources/server/`，均不提交。
下载后用 Linux `sha256sum -c SHA256SUMS` 或 macOS `shasum -a 256 -c SHA256SUMS` 校验。

## 兼容性与失败恢复

新应用沿用 Ait 正式应用 ID `dev.ait.desktop`。独立 server 默认使用
`~/.ait-server-desktop`；旧桌面的 SQLite 数据保留，但本次不自动迁移到新 server。

- 版本或包内容验证失败：修复 manifest、锁文件或暂存输入，重新运行门禁。
- 任一平台构建、签名或启动失败：Release job 不会运行。
- 发布阶段失败：在同一不可变标签上手动重跑，补齐资产。
- 已公开的错误版本：发布新的补丁版本，不移动标签。
