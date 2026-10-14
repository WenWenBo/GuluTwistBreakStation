# 咕嘟扭扭放风站

**咕嘟一口，扭扭两下，放风三分钟。**

离线桌面提醒工具：喝水、活动颈肩、按需如厕、站立与护眼。Rust + GPUI Kit，护眼默认全屏休息，其余默认轻提醒；每项可独立调整。

## 本机运行

当前个人预览版为 0.1.0（Mac build 7，菜单栏／托盘常驻），目标为 macOS 15+（Apple Silicon / Intel）与 Windows 10／11 x64。实际验证范围见[常驻模式报告](docs/validation/RESIDENT_APP_REPORT.md)，源码支持不等于目标机器实测通过。

macOS 打包后运行，系统通知需要真实 `.app`：

```bash
scripts/package-macos.sh aarch64-apple-darwin
open 'dist/aarch64-apple-darwin/GuluTwist Break Station.app'
```

Mac ZIP 文件名包含构建号（例如 `GuluTwist-0.1.0-build7-aarch64-apple-darwin.zip`），旧无构建号下载路径也同步为最新包。build 3 修复替换应用后 Dock 仍显示旧图标的问题，见 [修复记录](docs/validation/DOCK_ICON_FIX.md)。

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

Windows 11 已完成原生构建、便携包运行与主要常驻生命周期验证，并修复重复启动时的文件锁错误；托盘系统区域交互、Explorer 恢复、DPI、安装版与 Windows 10 仍待补验，详见 [Windows 验证报告](docs/validation/RESIDENT_APP_WINDOWS_REPORT.md)。

## 开发与检查

macOS 开发时，在项目根目录（包含 `Cargo.toml` 的目录）执行：

```bash
./scripts/dev.sh
./scripts/check.sh
```

`./scripts/dev.sh` 会编译并启动开发版，默认将测试设置和记录保存到 `dist/dev-data`，同时开启终端诊断日志。也可通过 `GULU_DATA_DIR` 指定其它测试数据目录。

启动脚本会自动切到项目根目录，因此也支持从任意目录通过脚本的绝对路径执行。

修改代码后需退出应用，再次执行启动命令；目前不支持热更新。在启动终端按 `Ctrl+C` 或从应用托盘退出即可停止。关闭主窗口只会隐藏窗口；同一数据目录已有实例时，再次启动只会激活旧实例。

开发启动不提供 macOS 通知身份，验证系统通知请使用 `.app`。打包应用也可使用独立测试数据与诊断日志：

```bash
GULU_DATA_DIR="$PWD/dist/manual-test" GULU_DIAGNOSTICS=1 \
  'dist/aarch64-apple-darwin/GuluTwist Break Station.app/Contents/MacOS/gulu-twist' \
  2>dist/manual-test.log
```

诊断默认关闭，只在标准错误输出应用命令、会话事件、遮罩数量，窗口可见性、常驻模式，以及锁屏/唤醒前后的时钟与剩余间隔，不采集键鼠、截图或其它应用内容。不要把测试数据库当作正式数据交付。

## 视觉更新

当前采用原创「小咕」圆润水团图标和暖白绿色界面。五项提醒首页、可折叠侧栏、分秒设置、未保存保护及多屏休息遮罩已更新。实际结果见 [视觉回归报告](docs/validation/UI_REFRESH_REPORT.md)，图标母版与重建步骤见 [资源说明](assets/icons/README.md)。

## 菜单栏／托盘常驻

Mac 正常运行只在菜单栏显示入口，打开设置也不占 Dock 或 ⌘Tab。Windows 在主窗口隐藏时只保留托盘；主窗口打开／最小化时保留任务栏与 Alt+Tab，托盘左键打开小站、右键打开菜单。图标可能位于系统的展开区。

首次引导保存成功后收起主窗口；之后冷启动不弹主窗口，胶囊按已有开关显示。关闭主窗口一次即隐藏，草稿仍保留；再次启动同一数据目录的应用可找回窗口。休息／锁屏时请求打开会延后；显式退出会清理所有窗口与图标。图标不可用时保留主窗口，提供重试与退出入口，Mac 临时恢复 Dock。

独立的 `.github/workflows/verify.yml` 在 main 的 PR 或手动触发时检查三平台构建，不发布 Release。本轮不改变 release 分支。完整方案与手工场景见[实现计划](docs/implementation/RESIDENT_APP_PLAN.md)和[验证手册](docs/validation/RESIDENT_APP_MANUAL_TESTS.md)。

## 桌面胶囊

在“提醒设置 → 桌面悬浮窗”开启 **显示桌面悬浮窗**，即可使用 200 × 52、两端半圆的小窗，查看最近事项和倒计时。点击图标或时间打开主窗口；关闭主窗口后胶囊继续显示。两者可以同时存在，隐藏胶囊不暂停提醒。拖动会记住位置，右侧更多菜单提供暂停、设置、重置位置与隐藏入口，托盘可重新显示。

旧用户默认关闭；开关与位置独立保存，不提交提醒规则草稿。正式休息、体验及锁屏期间临时隐藏，结束后按开关恢复。自动检查与 Mac 双架构打包已通过；Mac 本机已验证胶囊外观、主窗切换、拖动、开关与位置持久化、草稿保护以及休息隐藏／恢复。Windows 11 已补验原生构建、胶囊开关持久化与安静冷启动；Intel 实机运行、锁屏睡眠、混合缩放与稳定性等边界仍需补验。详见[胶囊验证报告](docs/validation/FLOATING_WIDGET_REPORT.md)、[Windows 验证报告](docs/validation/RESIDENT_APP_WINDOWS_REPORT.md)与[专项手工步骤](docs/validation/FLOATING_WIDGET_MANUAL_TESTS.md)。

## 代码与文档

- `gulu-core`：纯 Rust 配置、双时钟、工作时段与提醒状态机。
- `gulu-storage`：SQLite 事务、记录去重、设置备份和恢复。
- `gulu-app`：GPUI 界面、托盘、原生时钟/锁屏、多屏遮罩与后台存储。
- [使用说明](docs/user-guide.md)
- [其它机器验证步骤](docs/validation/MANUAL_TEST_GUIDE.md)
- [架构决策](docs/architecture/decisions.md)
- [OTA 自动更新文档](docs/ota/README.md)（需求已确认：Velopack + GitHub Releases、全量更新、草稿手动发布；尚未实现）
- [需求文档](docs/requirements/PRD.md) / [实施追踪](task_plan.md)
- [紧凑胶囊悬浮窗实现计划](docs/implementation/FLOATING_WIDGET_PLAN.md)（已实现，本机主要交互已验证）

当前版本没有账号、云同步、自动更新或开机启动。本轮 OTA 按无正式平台证书的预览版条件实现，不接入额外的更新签名验证；平台和更新实机验收仍待完成。正式代码签名与公证可在后续正式发行时接入。
