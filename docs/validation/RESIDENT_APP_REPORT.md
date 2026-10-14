# 菜单栏／托盘常驻交付报告

日期：2026-10-14。版本：0.1.0，macOS build 6。分支：`codex/bugfix`。实现提交：`c17a573`；Windows 启动锁屏修复：`ec685eb`。基线：`758483f`。

## 实现结果

- macOS 以 LSUIElement 和 GPUI Accessory 启动，正常开设置仍采用菜单栏模式；托盘失败临时恢复 Regular 和主窗入口，重试成功恢复 Accessory。
- Windows 主窗口保留普通任务栏／Alt+Tab 语义；胶囊、菜单、提醒条和遮罩隐藏创建、设置工具窗口样式后再展示。托盘左键开窗、右键菜单，提供设置快捷入口。
- 已引导用户冷启动不再取决于胶囊开关，主窗默认收起；首次引导保存成功后才隐藏。一次关窗即隐藏，重开复用窗口、还原最小化和保留草稿。
- 通知／托盘导航沿用未保存保护；休息或锁屏时暂缓打开，最后明确目的地优先于普通重开，退出丢弃请求。
- 注册健康检查纳入已有事件循环，最长 30 秒且关窗前再次确认；Windows 使用 NIM_MODIFY 返回值，复用 tray-icon 的 Explorer 恢复，不重复安装全局回调。注册不可用时保留主窗重试／退出入口；主窗和图标同时失败则原生提示后清理退出。
- 退出共用幂等收尾与持久化屏障；Windows 在启动开窗前同步读取锁屏状态，并在注册会话事件后再次读取以覆盖初始化间隙。
- 无配置 schema、标识、数据目录或默认规则变更；没有添加服务、自启项或第二个进程。既有紧急退出文字和按键限制保持。

上述为实现范围。系统 Dock／焦点／任务栏等行为仍须以下未测矩阵实机确认。

## 已执行验证

环境：Apple M4／arm64，macOS 15.7.3（24G419）；Rust 1.97.1，GPUI Kit 0.7.3、gpui-pre 0.3.9、tray-icon 0.24.2。

| 检查 | 实际结果 | 范围 |
| --- | --- | --- |
| `./scripts/check.sh` | 通过 | fmt、56 项测试（应用 12、核心 31、存储 13）、Clippy `-D warnings` |
| 新常驻策略测试 | 3 项通过 | 引导／入口／异常启动组合；锁屏与休息期间延后、只消费一次；明确导航保留及退出取消 |
| Windows 适配检查 | 通过 | 真实 `windows.rs`、`tray.rs` 与锁定 tray-icon API；GPUI 几何／窗口为桩，非完整 GUI 构建 |
| arm64 Release `.app` / ZIP | 构建通过 | 架构识别、ad-hoc 签名验证、图标、应用身份及 LSUIElement 校验 |
| x86_64 Release `.app` / ZIP | 交叉构建通过 | 同上；未在 Intel 真机运行 |
| 文档与差异检查 | 完成 | 当前 PRD、用户指南、UI 规范、架构决策、计划／报告同步；历史报告保持原记录 |

Windows 检查命令：

```bash
cargo check --locked --manifest-path tools/windows-adapter-check/Cargo.toml \
  --target x86_64-pc-windows-msvc
```

首次执行发现检查工具的 GPUI 桩缺失已有胶囊使用的几何和显示器类型；已补齐，同时将实际托盘模块纳入检查，最终通过。检查工具有独立 Cargo.lock，未改变应用依赖锁。

本轮检查中只有既有依赖 `block 0.1.6` 的 future-incompat 提示，不影响当前工具链通过；没有把它误报为本轮业务失败。

## 本地产物

包由 `c17a573` 的 Mac 源码构建；其后 `ec685eb` 仅修改 Windows 专用适配，不改变 Mac 二进制。包仍为个人 ad-hoc 签名，未公证。

| 架构 | 文件 | SHA-256 |
| --- | --- | --- |
| arm64 | `dist/GuluTwist-0.1.0-build6-aarch64-apple-darwin.zip` | `eddc49cc716f1c35198ffc17920ac45a70b819099592de794ce2a11d93414de8` |
| Intel x86_64 | `dist/GuluTwist-0.1.0-build6-x86_64-apple-darwin.zip` | `2cee9af843be107ad3b7c5580c34e1c361996935d6b47aec19be850a320d9849` |

解包后的本地应用分别位于 `dist/aarch64-apple-darwin/GuluTwist Break Station.app` 和 `dist/x86_64-apple-darwin/GuluTwist Break Station.app`。dist 不纳入 Git。

## 待测与执行限制

当前 Mac 处于锁屏状态。原生桌面工具实际返回 `The Mac is locked and automatic unlock could not unlock it`，因此本轮没有启动新包执行图形交互验证，也没有修改日常应用数据／进程。另有浏览器认证错误，与本应用结果无关。

| 项目 | 状态与原因 |
| --- | --- |
| C-01～C-14 原生交互 | 待执行；只有纯策略／引擎／存储部分已自动验证，不能将测试数当成桌面交互通过 |
| M-01～M-05：无 Dock、窗口编辑、Space／多屏、失败恢复、正式包通知 | 待解锁实测；本轮元数据与构建不能代替这些证据 |
| Intel Mac 真机 | 无可用设备，待执行；交叉构建不代表 Intel 运行通过 |
| Windows 完整 GUI 编译与安装包 | 本机无 Windows 原生 SDK／运行环境；仅已通过适配类型检查，未生成本轮 Windows 可执行包 |
| W-01～W-06：Win 10／11、Explorer 重启、DPI、安装／便携 | 待外机实测，不能声明已全面兼容 |
| 故障注入的原生行为 | 提供 tray／tray-once／tray-registration／main-window 测试入口和步骤；尚未执行实际点击恢复／退出 |
| 性能对比及 8 小时长稳 | 未执行；不宣称 CPU／内存降低或长期稳定性达标 |
| 验证 CI | 已配置 PR main／手动三平台验证，尚未远程运行；无发布步骤，release 工作流未改 |

继续验证使用 [手工回归步骤](RESIDENT_APP_MANUAL_TESTS.md)，包括隔离启动、各平台行为、故障解除、记录模板和性能口径。源码实现与本机可执行检查已交付，完整跨平台验收仍待上述证据。

## 分支与交付

本轮代码及文档在 `codex/bugfix` 本地提交，未合并 main／release、未推送或发布。此前 `758483f` 已合入本地 main 的事实保持；本次新功能不复用此前合并授权。
