# 菜单栏／托盘常驻交付报告

日期：2026-10-14。版本：0.1.0，macOS build 7。分支：`codex/bugfix`。实现提交：`c17a573`；Windows 启动锁屏修复：`ec685eb`。基线：`758483f`。本次解锁后补验发现并修复 Mac 原生错误弹窗崩溃。

## 实现结果

- macOS 以 LSUIElement 和 GPUI Accessory 启动，打开设置仍保持该模式；托盘失败临时恢复 Regular 和主窗入口，重试成功恢复 Accessory。
- Windows 主窗口保留普通任务栏／Alt+Tab 语义；胶囊、菜单、提醒条和遮罩隐藏创建、设置工具窗口样式后再展示。托盘左键开窗、右键菜单，提供设置入口。
- 已引导用户冷启动默认收起主窗，胶囊由原开关决定；首次引导保存成功才隐藏。关窗即隐藏，重开复用实体、还原最小化并保留草稿。
- 通知／托盘导航沿用未保存保护；休息或锁屏时暂缓打开，明确目的地优先于普通重开，退出丢弃请求。
- 注册健康检查纳入已有事件循环，最长 30 秒且关窗前再次确认；Windows 使用 NIM_MODIFY 返回值及 tray-icon 的 Explorer 恢复。失败保留主窗重试／退出入口；双入口失败则原生提示后清理退出。
- 退出共用幂等收尾与持久化屏障；Windows 在首次开窗前同步读取锁屏状态。
- 无配置 schema、应用身份或默认规则变更，没有新增服务、自启项或第二个常驻进程。

## 自动化与构建

环境：Apple M4／arm64，macOS 15.7.3（24G419），3 块显示器；Rust 1.97.1，GPUI Kit 0.7.3、gpui-pre 0.3.9、tray-icon 0.24.2。

| 检查 | 实际结果 | 范围 |
| --- | --- | --- |
| 修复后的 `./scripts/check.sh` | 通过 | fmt、56 项测试（应用 12、核心 31、存储 13）、Clippy `-D warnings` |
| 常驻策略测试 | 3 项通过，原有 53 项无回归 | 启动组合；锁屏／休息延后且只消费一次；明确导航保留和退出取消 |
| Windows 适配检查（实施阶段） | 通过 | 真实 windows.rs、tray.rs 与锁定库 API；GPUI 窗口是桩，非完整 GUI 构建 |
| arm64 Release build 7 | 构建并原生运行 | 架构、ad-hoc 签名、图标、身份及 LSUIElement 校验通过 |
| x86_64 Release build 7 | 交叉构建通过 | 同样完成包校验；未在 Intel 真机运行 |

Windows 检查命令：`cargo check --locked --manifest-path tools/windows-adapter-check/Cargo.toml --target x86_64-pc-windows-msvc`。当前只保留既有 `block 0.1.6` future-incompat 提示，没有业务检查失败。覆盖率、像素基线和突变测试未配置，本轮没有执行。

## 解锁后的原生实测

使用独立目录 `dist/resident-validation-20261014`，通过 CUA 操作真实 Release 应用、原生窗口和 SQLite 数据验证。build 6 完成前半轮交互，build 7 验证修复及受影响流程；下表区分版本，不把旧包结果冒充新包重测。

| 场景 | 版本与结果 | 证据（相对于上述目录） |
| --- | --- | --- |
| 首次引导与默认规则 | build 6／7：显示引导，30／40／90／60／20 分钟及“休息”文案正确；build 7 明确验证保存落盘后窗口自动隐藏 | `05-onboarding.png`、`14-onboarding-final.txt`、`onboarding-final.log` |
| 冷启动，胶囊开／关 | build 6：两种情况主窗均未创建，胶囊与开关一致；实际 NSApplication 策略为 Accessory | `timed.log`、`floating-cold.log` |
| 关闭、重开、最小化还原 | build 6：一次关闭隐藏；重复启动复用原进程和窗口，还原最小化 | `runtime.log`、`04-restored-settings.png` |
| 未保存草稿与导航 | build 6：隐藏／最小化后草稿保留；离开设置出现保存／放弃选择 | `03-draft-navigation.txt`、`04-restored-settings.png` |
| 重复生命周期 | build 6：20 次连续隐藏／重开，每次只有一个主窗；未测试 20 次正式休息 | `06-window-cycles.json`、`06-after-cycles.txt`、`timed.log` |
| 胶囊入口 | build 6：仅胶囊冷启动，菜单进入设置，主窗与胶囊共存，仍为 Accessory | `floating-cold.log` |
| 体验休息 | build 6：发送 Esc、Backspace、⌘W 后遮罩仍在；点击灰色下划线文字撤下 3 个遮罩，活动表仍为空 | `07-preview-after-keys.txt`、`08-preview.png`、`timed.log` |
| 正式自然结束 | build 6：30 秒护眼休息撤下 3 个遮罩，SQLite 恰有一条 Timed；当时主窗可见 | `timed.log`、`timed/station.sqlite3` |
| 首次托盘失败与重试 | build 6：Regular 兜底；关闭不隐藏最后入口；重试后 tray=true、Accessory，主窗保留；再关闭可隐藏 | `09-tray-fallback.png`、`fault-once.log` |
| 持续注册失败 | build 6：重试不会假报成功，关闭保留主窗，退出按钮正常结束进程 | `10-registration-fallback.txt`、`fault-registration.log` |
| 双入口同时失败 | build 6 复现崩溃；build 7 修复后原生提示可确认，清理退出，未再发生 panic | `fault-both.log`、`11-combined-failure-fixed.txt`、`fault-both-fixed.log` |
| 仅主窗失败 | build 7：原生提示、重复打开可再试、进程和 tray 保持；提示确认后 ⌘Q 正常退出 | `12-main-failure-fixed.txt`、`fault-main-fixed.log` |
| 纯后台到期与结算 | build 7：无主窗、无胶囊，真实 5 分钟后喝水轻提醒与三屏护眼休息出现；紧急退出后全部撤罩，完成喝水后两项各记录一次，主窗仍未创建 | `13-background-reminder.txt`、`background-final.log`、`background-final/station.sqlite3` |

工具读取隐藏应用的 AX 树可能产生系统 Reopen，甚至重新启动已退出应用，因此隐藏与退出结论以进程、原生可见性日志和数据库为依据，避免把工具唤起误判成应用主动弹窗。`main_exists=false` 与 `main_visible=false` 分别代表未创建和不可见；NSApplication policy 1 为 Accessory，0 为 Regular。20 次循环 JSON 的两次标题匹配包含“当前焦点”行，不代表两个窗口。

纯后台正式护眼休息的 Interrupt 在 CUA 介入前已出现在日志中，确认撤罩和唯一 Interrupted 记录；不将该动作写成代理亲自点击。喝水完成由 CUA 点击并核对唯一 Manual 记录。全部本轮测试进程已正常退出。

状态栏图标自身的菜单点击尚未取得稳定工具证据；已操作到的顶部“打开／关闭／退出”是应用菜单，不能冒充托盘菜单。多屏日志确认创建／撤下 3 个遮罩，截图仅证明被捕获屏幕的布局。hover 手形没有原生指针图像证据，未记为实测通过。

## 本次发现与修复

组合注入 `GULU_TEST_FAILURES=tray,main-window` 在 build 6 触发 `RefCell already borrowed`。崩溃堆栈显示：控制器持有 GPUI App 借用时同步调用 NSAlert.runModal，Cocoa 嵌套事件循环触发输入法／键盘布局回调，再次借用 App 导致 abort。

修复将原生错误提示调度至独立的主线程异步任务，在不持有 App／Controller 借用时展示；提示期间抑制重复开窗／弹窗，确认后发回事件，在没有托盘入口时走原有清理退出流程。复验双入口失败和主窗单独失败均无崩溃。诊断开关额外记录主窗口可见性、辅助窗口数及 Mac 激活策略；默认关闭。

原始崩溃报告：`~/Library/Logs/DiagnosticReports/gulu-twist-2026-10-14-141229.ips`。可重复注入步骤见[手工回归](RESIDENT_APP_MANUAL_TESTS.md)。本次运行证据保存在 `verification.ndjson` 和 `runtime.ndjson`，失败证据保留，不以修复后的通过覆盖。

## 本地产物

build 7 包包含本次弹窗修复和诊断日志。个人 ad-hoc 签名，未公证；dist 不纳入 Git。

| 架构 | 文件 | SHA-256 |
| --- | --- | --- |
| arm64 | `dist/GuluTwist-0.1.0-build7-aarch64-apple-darwin.zip` | `9bb2a7d9f623c1f5e0be239df6aee7515e041106e19b168d4584a17043ebc76a` |
| Intel x86_64 | `dist/GuluTwist-0.1.0-build7-x86_64-apple-darwin.zip` | `7e408454bc1db4af93764a0ede291eebefc578fd5b4d644682de38bd01604f08` |

解包后的应用分别位于 `dist/aarch64-apple-darwin/GuluTwist Break Station.app` 和 `dist/x86_64-apple-darwin/GuluTwist Break Station.app`。

## 待测边界

| 项目 | 实际限制 |
| --- | --- |
| Dock／⌘Tab 视觉与启动闪现 | 已验证 LSUIElement、实际 Accessory 和失败切换；Dock／系统 UI 工具未取得有效画面，不能判完整视觉验收通过 |
| Finder／Spotlight、状态栏菜单 | 已使用 LaunchServices 启动及单实例命令唤起，未完整测试 Finder／Spotlight 操作及状态图标菜单 |
| 通知／Space／完整多屏 | 正式包通知点击保护、全屏 Space、逐屏退出与焦点恢复待测 |
| 系统与异常边界 | 本版本休息／锁屏期间的原生延后打开、真实睡眠、热插拔、全部关闭／下班冷启动、保存失败与保存中退出等未全覆盖；延后逻辑仅已通过确定性测试 |
| Intel Mac | 无设备；交叉构建不能代替真机运行 |
| Windows | 无原生 SDK／运行环境，本轮没有 Windows 完整 GUI 包；Win 10／11、Explorer 恢复、DPI、安装／便携均待外机验证 |
| 性能与 8 小时长稳 | 未执行对比采样或 8 小时测试，不宣称 CPU／内存降低或长期稳定性达标 |
| CI | 已配置只验证不发布的工作流，尚未远程执行 |

按上述结果，本机主要交互已获得原生证据，完整跨平台验收仍未完成。本轮修改在 `codex/bugfix` 本地提交，未合并 main／release、未推送或发布。
