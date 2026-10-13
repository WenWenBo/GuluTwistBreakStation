# 小咕图形资源

原创 SVG 母版位于 `source/`。当前采用 A「伸懒腰的小水团」：绿色圆润底板、透明四角、奶白水团和一抹杏色。B「抱杯小水团」、C「小小放风站」保留为探索稿，未用于应用。

- `app.png`：1024×1024 RGBA；底板占画布约 82%，四角和外边距透明。
- `sizes/`：16、20、24、32、40、48、64、128、256、512、1024 px。
- `app.ico`：包含 16–256 px 共九档，供 EXE 资源、安装器和快捷方式使用。
- `mascot.png`：界面上的小咕，无底板。
- `tray-template-18.png`、`tray-template-36.png`：macOS 单色模板，跟随菜单栏亮暗显示。
- `tray-macos.rgba`、`tray-windows.rgba`：托盘 API 使用的未预乘 RGBA，尺寸分别为 36、32。

重建：在项目根目录运行 `npm ci --prefix tools/icon-export`，然后 `python3 scripts/generate-icons.py`。依赖 sharp 固定在锁文件；运行程序或正常 Rust 构建不需要 Node。修改 SVG 后须重建资源；macOS 的 ICNS 由 `scripts/package-macos.sh` 生成。打包使用全新目录，按 ICNS 内容 SHA256 命名 `AppIcon-<12位摘要>.icns` 并写入 `CFBundleIconFile`；应用启动时也从内嵌 `app.png` 设置 Dock 图标，避免继续使用旧系统缓存。脚本在交付前运行 `scripts/verify-macos-icon.py` 核验尺寸、引用和目录时间。

`assets/ui/` 是配套的圆头 1.8 px 描边图形：喝水、颈肩、放风、站立和导航，不依赖 Emoji 字体。
