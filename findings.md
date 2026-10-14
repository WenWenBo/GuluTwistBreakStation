# 事实、决策与待验证事项

更新日期：2026-10-13。

## 已确认的需求

- 用户已确认 [PRD 1.0](docs/requirements/PRD.md)，本轮要求编写实施计划。
- 个人使用、离线免登录，包含喝水、颈肩、如厕、站立四类提醒。
- 默认强制遮罩和倒计时，可切换轻提醒；紧急退出必须立即可用。
- 提醒间隔仅累计有效工作时间；锁屏、休眠暂停间隔。已开始的休息倒计时计入锁屏/休眠经过时间。
- Windows 10 与 Intel Mac 是明确要求；本机 Apple Silicon Mac 也必须可用。
- PRD 包含 14 个用户故事、16 条计时规则、10 项非功能要求、23 个验收场景；US-13、US-14 为 P1，其余故事为 P0。

## 工作区与环境事实

- 编写实施计划前，项目文件只有 `docs/requirements/PRD.md`，尚无 Rust 应用清单或源码。
- 上一轮只读检查：macOS 15.7.3（24G419）、arm64、Apple M4。这里只继承已核实信息，未把它当作应用运行结果。
- 当前没有 Windows 10 和 Intel Mac 的应用运行证据，也未确认对应实机已可用。
- 规划技能的会话恢复脚本已执行，没有返回需要补入的旧规划状态。

## 实施决策

| 决策 | 理由 | 验证方式 |
| --- | --- | --- |
| 优先验证 GPUI Kit | 前序选型建议；有现成控件和通知接入文档 | 用固定版本在三个目标架构上运行最小打包程序 |
| 核心逻辑独立于 GPUI | 双时钟、恢复、去重与合并应可独立测试 | 纯 Rust 引擎不依赖窗口库，可注入时钟 |
| 先验证退出和多屏，再扩展界面 | 遮罩是核心交互，失败会阻碍使用 | 单屏、双屏、热插拔和异常撤下实机测试 |
| 本地 SQLite 为存储优先方案 | 设置与记录需要事务、去重和迁移 | 事务与故障恢复集成测试 |
| 先形成一条端到端闭环 | 能尽早发现事件、窗口与存储间的问题 | 一项提醒到期、结束/退出、记录、重启恢复 |
| 使用三个 crate 的依赖结构 | 保持核心与存储独立，控制小工具的工程复杂度 | `gulu-app` 组合 `gulu-core` 和 `gulu-storage`，不反向依赖 UI |
| 26 项核心任务与 3 项后续增强任务 | P0 需求完整落地，P1 功能保持明确边界 | `task_plan.md` 包含任务依赖和全部 PRD 编号追踪 |
| 测试设备缺失不等于兼容通过 | 当前仅有本机环境证据 | 设备缺失时继续独立模块，保留平台任务 blocked 与发布门槛 |

## 待验证技术假设

1. 所锁定 GPUI Kit 版本及 GPU 后端兼容 Windows 10 和 macOS 15 Intel；不能仅凭平台名称判定。
2. 托盘/菜单栏、关闭窗口保活、多屏遮罩、Space/虚拟桌面与通知点击路由能够一起工作。
3. 各平台提供可计入休眠且不受手动调时影响的连续时钟；系统事件漏发/重复时仍能正确恢复。
4. Release 空闲资源占用达到 PRD 预算；活动多屏遮罩占用与后台空闲需分别测量。
5. 通知必须以真实应用身份和 `.app`/Windows 安装包测试，API 提交成功不等于系统展示。

## 来源

- 已补充[跨平台专项验证方案](docs/validation/PLATFORM_VALIDATION_PLAN.md)：规定 6 项基础、9 项遮罩、10 项双时钟检查，区分规则自动化与真实系统证据；当前均未执行。
- [需求文档](docs/requirements/PRD.md)
- [GPUI Kit 安装指南](https://gpui-kit.com/docs/installation)
- [GPUI Kit 系统通知](https://gpui-kit.com/docs/system-notification)
- [GPUI Kit 打包指南](https://gpui-kit.com/docs/packaging)

官网结论沿用需求阶段的 2026-10-13 查阅记录；实际启动开发时核对所选版本，不默认为浮动最新版。

## 2026-10-13 实施更新（覆盖前述规划期状态）

- 已按用户最新要求实际实施，外机验证后移，不阻塞本机开发。
- GPUI Kit 0.7.3 / gpui-pre 0.3.9 在 M4 已构建运行；Intel x86_64 完整 Release 构建/打包通过。Windows 适配代码类型检查通过，完整 SDK 构建需 Windows 机器。
- 当前设备连接三块显示器，已创建三屏遮罩；真实四项五分钟提醒合并为 15 秒，结束数据库为四条同场 Timed。
- 当前本机被锁屏打断界面回归。算法与存储自动化、包、文档继续完成；最终 UI、通知展示、真实睡眠/热插拔仍待验证。
- 新决策以 `docs/architecture/decisions.md` 为准；证据与已知边界见 `docs/validation/LOCAL_REPORT.md`，外机操作见 `docs/validation/MANUAL_TEST_GUIDE.md`。


## 2026-10-13 · 视觉实现发现

- 旧应用 PNG alpha 全为 255；新图透明区 35.6%，Mac 直接读取透明 ICNS，Windows 使用九尺寸 ICO。
- GPUI 0.3.9 在 Mac 的 `titlebar: None` 仍设置 `NSTitledWindowMask | NSFullSizeContentViewWindowMask`，不是 Borderless。实测视口 1470×956，原生 frame 起点 y=38（外屏 y=25）；底部在屏外。原生 `setStyleMask(Borderless)` 后全屏原点 y=0，按钮 / 提示完整，Esc 可用。
- 倒计时换专用等宽字体、常规字重与显式行高后本机可读；没有证明旧字形缺损唯一来自某条 CoreText 路径，不将猜测写为已证根因。
- 图形导出工具 sharp 固定版本，SVG 源与生成资源提交式保存，Rust 运行 / 正常打包无需 Node。

## 紧凑胶囊补验（2026-10-13 晚间）

- 用户已解锁。本轮 build 4 本机主要交互可验证，先前锁屏阻挡已解除；具体通过子项与边界以 `docs/validation/FLOATING_WIDGET_REPORT.md` 为准。
- CUA 脚本拖动未观察到位移，但用户实体鼠标拖动成功、无误开主窗；不将自动化限制当作产品缺陷。
- 本机实际连接三块屏幕（内置屏 + 两块 E24-30）。自动截图在外接屏出现缩放留白，用户再次手动体验确认三屏完整覆盖、内容居中和退出按钮可见。无需根据单张工具截图盲改窗口代码。
- CUA 在进程退出后读取界面可能重新启动应用并使用日常数据。测试重启须先确认进程结束，再用同一隔离数据目录显式启动，避免误改日常设置。


## 2026-10-14 · 菜单栏／托盘常驻规划调查

- 代码基线 `758483f`；`main` 与 `codex/bugfix` 同提交。本次开始时工作区干净。
- 本机 PixPin 的 `Info.plist` 已读到 `LSUIElement = true`；本项目未配置该标志。GPUI Kit 0.7.3 / gpui-pre 0.3.9 默认激活策略为 Regular。
- `App::set_activation_policy(Accessory)` 支持可显示窗口的菜单栏工具；`Application::with_windowing` 能配置启动策略，需要继续核对窗口创建能否保持该策略。
- 当前冷启动以 `!floating_visible` 作为打开主窗口的条件，导致关闭胶囊时不能纯托盘启动。`main_visible` 初始为 true，纯托盘启动应改成真实窗口状态。
- 当前首次关窗只显示常驻说明，第二次才隐藏；新方案需要一次关闭即隐藏，并将说明放到引导／设置。
- Windows 胶囊与提醒条已使用 TOOLWINDOW 并清除 APPWINDOW；全屏遮罩尚无同样处理。托盘创建失败目前能打开主窗，但 Mac 改为 Accessory 后还须恢复 Regular。
- `wait_ms` 已区分可见倒计时（最长 1 秒）和后台（最长 30 秒），主窗口不可见时不调用其 notify；不能宣称隐藏 Dock 本身降低内存。
- 现有 CI 仅在 `release` 分支推送时构建三平台并发布；验证新模式不能通过合并 release 来触发。

- GPUI 启动方案已明确：正常 `application()` 配合仅 macOS 的 `with_windowing(WindowingRequest::Headless)` 会将初始策略设为 Accessory；这里是窗口参与系统界面的模式，不是创建没有图形能力的平台。当前 `open_window` 路径不调用 `request_windowing`，不会自行切回 Regular，仍需正式 .app 实测。
- Cargo.lock 锁定 `tray-icon 0.24.2`，其 Windows 实现已有 `TaskbarCreated` 重新注册；初次注册失败可能保留句柄等待 Explorer，而不是使 `build()` 报错。因此 `tray.is_some()` 只表示对象存在，不能完全代表系统托盘已注册。需要将这项差异纳入探针、失败兜底及重启 Explorer 验收。
- 通知点击 `Event::Notice` 当前调用 `MainView::saved()`，会改变页面并清理导航状态；常驻重开应走 `navigate()` 的草稿保护路径。
- 先前按猜测版本搜索 tray-icon 目录失败，已通过 Cargo.lock 和文件发现定位 0.24.2；不按旧版本推断行为。


## 2026-10-14 · 常驻实现与验证结论

- GPUI Accessory 的启动策略与 plist 已同时落地；主窗恢复显式还原最小化，窗口实体复用保留草稿，通知导航不再调用 saved()。
- 托盘事件库全局处理器仅能初始化一次，因此在构建图标之前独立 init，重试只替换图标对象。Windows 注册确认复用 set_tooltip，既有 TaskbarCreated 恢复不另造第二套。
- wait_ms 必须包含健康检查的绝对截止点，避免其他事件重启睡眠后把约 30 秒检查拖到近 60 秒；没有新增高频后台线程。
- Windows 初始锁屏状态原在异步会话线程中读取，可能晚于开窗；现同步首读，注册事件后重读并发送 Tick。
- 适配检查原有 GPUI 桩缺少已有悬浮代码使用的几何／显示器类型，导致首轮失败；补齐并加入真实 tray.rs 后，最终 --locked 检查通过。这仍非 Windows 应用构建。
- 最终 56 项测试、fmt、Clippy 通过，Mac arm64／Intel build 6 构建验签及元数据检查通过；既有 block 0.1.6 future-incompat 提示保留。
- CUA 实际检查返回 Mac 锁屏无法解锁；本轮原生 UI、失败注入、Windows／Intel 真机和性能／8 小时检查均未执行，详见专项报告。没有以隐藏图标宣称资源下降。
