# 咕嘟扭扭放风站

**咕嘟一口，扭扭两下，放风三分钟。**

离线桌面提醒工具：喝水、活动颈肩、按需如厕与站立。Rust + GPUI Kit，默认多屏强制休息，随时可紧急退出，也可切换轻提醒。

## 本机运行

当前个人预览版为 0.1.0（Mac build 3），目标为 macOS 15+（Apple Silicon / Intel）与 Windows 10 x64。实际验证范围见[验证报告](docs/validation/LOCAL_REPORT.md)，源码支持不等于目标机器实测通过。

macOS 打包后运行，系统通知需要真实 `.app`：

```bash
scripts/package-macos.sh aarch64-apple-darwin
open 'dist/aarch64-apple-darwin/GuluTwist Break Station.app'
```

Mac ZIP 文件名包含构建号（例如 `GuluTwist-0.1.0-build3-aarch64-apple-darwin.zip`），旧无构建号下载路径也同步为最新包。build 3 修复替换应用后 Dock 仍显示旧图标的问题，见 [修复记录](docs/validation/DOCK_ICON_FIX.md)。

Intel Mac 包：

```bash
rustup target add x86_64-apple-darwin
scripts/package-macos.sh x86_64-apple-darwin
```

Windows 在装有 Visual Studio 2022 C++ Build Tools 和 Windows SDK 的机器上：

```powershell
./scripts/package-windows.ps1
```

生成安装包需要 Inno Setup 6；可加 `-SkipInstaller` 生成便携 ZIP。Rust 版本由 `rust-toolchain.toml` 锁定。macOS 构建需要 Xcode Command Line Tools。首次构建下载依赖，程序运行不需要网络。

## 开发与检查

```bash
cargo run -p gulu-app
scripts/check.sh
```

`cargo run` 不提供 macOS 通知身份，通知请使用 `.app`。独立测试数据与可选诊断：

```bash
GULU_DATA_DIR="$PWD/dist/manual-test" GULU_DIAGNOSTICS=1 \
  'dist/aarch64-apple-darwin/GuluTwist Break Station.app/Contents/MacOS/gulu-twist' \
  2>dist/manual-test.log
```

诊断默认关闭，只在标准错误输出应用命令、会话事件、遮罩数量，以及锁屏/唤醒前后的时钟与剩余间隔，不采集键鼠、截图或其它应用内容。不要把测试数据库当作正式数据交付。

## 视觉更新

当前采用原创「小咕」圆润水团图标和暖白绿色界面。默认四卡首页、可折叠侧栏、分秒设置、未保存保护及多屏休息遮罩已更新。实际结果见 [视觉回归报告](docs/validation/UI_REFRESH_REPORT.md)，图标母版与重建步骤见 [资源说明](assets/icons/README.md)。

## 代码与文档

- `gulu-core`：纯 Rust 配置、双时钟、工作时段与提醒状态机。
- `gulu-storage`：SQLite 事务、记录去重、设置备份和恢复。
- `gulu-app`：GPUI 界面、托盘、原生时钟/锁屏、多屏遮罩与后台存储。
- [使用说明](docs/user-guide.md)
- [其它机器验证步骤](docs/validation/MANUAL_TEST_GUIDE.md)
- [架构决策](docs/architecture/decisions.md)
- [需求文档](docs/requirements/PRD.md) / [实施追踪](task_plan.md)

首版没有账号、云同步、自动更新或开机启动。公开发布前还需要正式签名、公证与完整的平台和长期稳定性验收。
