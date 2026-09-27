# desktop

Electron desktop app for Ait. The current application version is `0.0.7`.

Run `npm run verify:release` at the repository root to check that desktop, mobile,
Web, local package manifests and lockfiles match the Rust workspace release.
The display name is Ait. Release builds use Ait's `dev.ait.desktop` application ID.
Existing profile directories and the `paseo` link protocol retain their compatibility names.

Official releases build this app for Linux x64 and macOS arm64. The only bundled
server/CLI resource is `resources/bin/server`; no legacy daemon, worker or CLI shim
is shipped. See [release operations](../../docs/operations/releasing.md).

从仓库根目录运行 `npm run build:dmg` 生成包含 Rust server 的本地 macOS 安装包。
正式签名、公证及产物路径见 [Apple 构建说明](../../docs/operations/apple-builds.md)。

### Settings regression against Rust

With the desktop development server running, run:

```sh
EXPO_DEV_URL=http://localhost:8082 npm run test:e2e:settings-rust --workspace=@getpaseo/desktop
```

The test launches a separate Electron profile and Rust data directory, visits all 20 settings pages,
checks project details, Skills and Agent profile editors, saves host and terminal configuration,
and opens the native Provider settings. It removes its temporary data when finished.
The server binary defaults to `target/debug/server`; set `AIT_SERVER_BIN` to use another build.
