# Dock 仍显示旧图标：build 3 修复

日期：2026-10-13。适用 macOS Apple Silicon / Intel；提醒、设置与存储逻辑未修改。

## 证据与原因

用户截图为运行中的 macOS Dock 旧圆脸图标，而非顶部菜单栏的单色托盘图标。

- 下载目录与工作区 build 2 ZIP 的 SHA256 都是 `b1df0b83a64a6d39c9ba5d5ab473381b636861567053f0ca8f887ded2dcee0e3`，应用二进制和 ICNS 也一致，排除拿错包或漏打资源。
- 下载目录应用的 Finder 简介小图与预览均显示新版小咕；ICNS SHA256 为 `e0768e63bc1cd9d13ce671b3a24613f6161df8cb1fe01f47f72bdcf7de8f2de7`。
- 原脚本原地复用 `.app`，应用目录时间仍为 11:16，图标文件已在 15:20 更新，`CFBundleIconFile` 一直为 `AppIcon`。
- 程序此前未设置 `NSApplication.applicationIconImage`，Dock 完全依赖系统对应用包的图标解析与缓存。

由此将问题定位到包更新后的系统图标显示 / 缓存路径，而非图稿或托盘 RGBA。具体是 Dock 还是 LaunchServices 的哪一级缓存未失效，现有证据不能进一步区分；没有通过清空系统缓存掩盖问题。

## 修复

1. macOS 初始化在主线程解码内嵌 `app.png`，调用 `setApplicationIconImage`。运行中的 Dock 图标直接来自当前程序资源；解码失败保留包图标并记录错误。
2. 打包使用新的临时目录构建完整应用，校验通过后再替换输出，避免旧资源与旧目录时间残留。
3. ICNS 按内容摘要命名为 `AppIcon-e0768e63bc1c.icns`，同步写入 Info.plist，更新目录时间。后续图稿变化时文件名自动变化。
4. 构建号升为 3，ZIP 名带 `build3`；此前分享的不带构建号下载路径也同步为 build 3。
5. 增加 `scripts/verify-macos-icon.py` 检查 ICNS 结构、各档尺寸、内容摘要、plist 引用、旧资源残留和目录修改时间。

## 验证

- 修复前新验证脚本对 build 2 报错：`icon filename must change with its content`；旧目录时间检查也为失败。
- build 3 的 Apple Silicon / Intel 构建、图标自检、签名验证通过；ZIP 解读确认为 build 3 且引用的 ICNS 存在。
- 37 项现有测试全部通过；fmt、workspace clippy `-D warnings` 通过。
- 已替换用户下载目录的 `.app`、原 ZIP，并另存带 build 3 名称的 ZIP；使用隔离目录启动替换后的应用，界面正常。
- `dist/dock-icon-fix/after.log` 包含 `dock_icon=embedded-small-gulu`，确认发布程序成功解码并执行原生 Dock 图标设置。
- CUA 获取 Dock 连续超时，无法取得其实际截图。不能将原生调用成功等同于 Dock 目视验收通过，仍需用户看到运行中的图标确认；无需重置整个 Dock 或系统缓存。

## 产物

| 架构 | ZIP | SHA256 |
| --- | --- | --- |
| Apple Silicon | `GuluTwist-0.1.0-build3-aarch64-apple-darwin.zip` | `ede54794bf124482324b9c16cf5996e4db157981a961af1250140edffb432827` |
| Intel | `GuluTwist-0.1.0-build3-x86_64-apple-darwin.zip` | `4a5ca89ccac3ebb380ab09b81587380bbed52b05994d4fd449e0ad1b890c2a90` |

两包位于 `dist/`。Intel 本轮完成交叉构建，仍未进行 Intel 真机运行验证。测试数据使用 `dist/dock-icon-fix/`，未修改日常提醒设置。
