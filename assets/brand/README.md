# AIT 飞鸟生产资产

采用 [ADR-051](../../docs/decisions/adr-051-ait-brand-identity.md) 中的 04 飞鸟。

唯一可编辑源是 [`ait-mark.json`](../../apps/app/src/branding/ait-mark.json)：
包含两个折面、合并轮廓、小尺寸光学校正轮廓、路径化 AIT 字标、纯色令牌及容器位置。
应用内 `AitLogo` 与导出脚本直接读取同一份数据。这里的 SVG、根目录 Logo 和各端 PNG、
ICO、ICNS 都是生成产物，请修改源后统一生成，不单独修改某一端文件。

```sh
npm ci
npm run generate:icons
npm run check:icons
```

生成依赖固定版本的 `sharp`，无需字体、图像生成服务或额外图标转换程序。
`check:icons` 在相同依赖环境下重新生成并逐字节检查 38 份产物，缺失或过期时返回失败。
`apps/desktop` 中原有的 `npm run generate:icons` 也调用同一脚本。

| 资产 | 用途 |
| --- | --- |
| `ait-mark.svg` | 双色独立飞鸟，透明底 |
| `ait-mark-mono.svg` | 深炭色单色轮廓 |
| `ait-mark-small.svg` | 小尺寸单色轮廓，16–32 px favicon |
| `ait-lockup-horizontal.svg` | 横向图形与路径化 AIT 字标 |
| `ait-lockup-stacked.svg` | 纵向组合，欢迎与启动场景 |
| 根目录 `logo.svg` / `logo.png` | 浅色圆角桌面图标、README 与旧桌面壳 |
| `apps/paseo/assets/` | Electron 开发/发行 PNG、Windows ICO、macOS ICNS、Linux 尺寸集 |
| `apps/app/assets/images/` | Expo 图标、自适应前景、通知、启动及 favicon 状态资源 |
| `apps/app/public/` | Apple touch 与 PWA 安装图标 |

iOS 图标和 PWA 图标是不透明方图；系统负责遮罩。Android 前景和通知图标保留透明背景，
通知图标为纯白轮廓。PWA 与 Android 的飞鸟比例分别服从各自安全圆；桌面圆角不用于移动端母图。
深色页面复用飞鸟，字标反白、favicon 使用珊瑚色保证识别，没有另立一套夜间品牌方向。
运行蓝点、提醒绿点属于应用状态，不属于品牌配色。

干净的 Expo prebuild 会读取新资源。已有本地 iOS prebuild 可显式同步：

```sh
npm run generate:icons -- --sync-native
npm run check:icons -- --sync-native
```

这会刷新现有 `Images.xcassets` 内的应用图标、启动图及启动底色，不提交生成的原生工程。
Android 使用项目已有的 clean prebuild 构建入口。更换已安装应用的系统图标需要重新构建和安装；
Electron 分支图标缓存版本已经更新。Web 构建使用带内容哈希的 favicon。

实际产物和页面截图见 [落地报告](../../docs/reports/ait-brand-rollout.md)。
