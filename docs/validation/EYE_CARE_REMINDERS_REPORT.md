# 护眼与分项提醒交付报告

日期：2026-10-13。应用 0.1.0，macOS build 5。分支：`feat/compact-floating-capsule`。

## 已实现

- 新增保护眼睛，默认开启、每 20 分钟全屏休息 20 秒；其它四项默认轻提醒。五项独立选择提醒方式。
- 轻提醒使用 200×40 圆角提醒条和圆形勾选按钮；附着于 200×52 胶囊，底部不足时整组翻到上方。胶囊保持下个未来事项倒计时。关闭胶囊后仍临时显示独立提醒条。
- 同项最多一个待办，不再两分钟过期；当前连续工作时段内保留，锁屏／睡眠／暂停／全屏期间隐藏，回来恢复。午休、下班或退出清空，重启不补发。
- 仅全屏事项参与全屏合并；轻提醒待办不随护眼自动完成。勾选只结算本次实例、重置本项间隔。提醒方式与时长修改在下一实例生效。
- 配置升级到 schema 2。旧四项统一轻提醒，保留启用、间隔、时长和记录，新增护眼；原 JSON 保存在 `settings_v1_backup`，事务失败不做部分升级。升级只执行一次。
- 提醒条不重复弹系统通知；创建失败才降级为系统通知和主窗口待办。Mac／Windows 继续共用既有无激活浮窗适配。

## 自动检查

| 检查 | 结果 |
| --- | --- |
| `cargo test --locked --workspace` | 53 项通过：core 31、storage 13、app 9 |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | 通过；依赖 `block 0.1.6` 有 Rust future-incompat 提示 |
| `cargo fmt --all -- --check`、`git diff --check` | 通过 |
| Apple Silicon Release 打包／签名／图标 | 通过 |
| Intel Mac Release 交叉构建／签名／图标 | 通过；未在 Intel 真机运行 |

新增规则测试覆盖默认 20／20、强轻同批分流、轻提醒无响应保留、重复完成、锁屏／睡眠／暂停保留、跨时段与退出清空、方式从下一实例生效，以及迁移仅一次／损坏／未来版本／只读失败。几何测试覆盖五条堆叠、底边翻转、负坐标工作区和独立条目边界校正。真实原生窗口创建故障没有做注入测试。

## 本机实测

在本机 Apple Silicon（M4）、最终 arm64 `.app`、独立 `dist/eye-care-test` 数据目录测试。先创建旧版 schema 1 四项配置：全天工作、四项间隔 5 分钟；正式启动执行迁移。正常用户数据未用于测试。

1. 设置页显示原四项轻提醒、护眼 20 分钟／20 秒全屏；SQLite 同时保留原 v1 JSON。通过界面把护眼间隔改为 5 分钟后正式等待，没有修改运行时钟或提前注入到期事件。
2. 20:51:54 四项同时轻提醒到期；胶囊继续显示护眼未来倒计时。20:53:26 护眼开始 20 秒正式休息，日志显示 `overlay_count=3`；本机主屏截图内容与退出入口完整。20:53:46 只产生一条 EyeCare / Timed，原四项待办仍保留。
3. 20:54:27 点击站立提醒条圆形勾选，仅记录站立 Manual；其它三项保留。20:57:10–20:57:33 逐条勾选放风、颈肩、喝水，每个原实例仅一条 Manual。待办已超过旧版两分钟期限，未自动过期或重复堆积。
4. 关闭胶囊后独立提醒条可继续完成，数据库的 `floating_enabled` 始终为 false。手动暂停后恢复，未完成条目重新出现，没有新增取消记录。
5. 有喝水待办时把喝水设为全屏，保存后当前待办仍可勾选，没有立即弹全屏。把护眼设为轻提醒后，下一次到期产生眼睛图标提醒条，未创建遮罩，勾选仅新增 EyeCare / Manual。

6. 21:00:58 有新的站立待办时正常退出，原实例记为 Cancelled / 应用退出，进程退出码 0。随后重启五项均从完整间隔开始，无旧待办；原 1 场休息／6 项完成记录、喝水全屏／护眼轻提醒以及关闭胶囊偏好均保留。最终测试进程再次正常退出。

截图见 [默认设置](evidence/eye-care/default-settings.jpg)、[正式护眼遮罩](evidence/eye-care/eye-overlay.jpg)、[站立提醒条](evidence/eye-care/reminder-row.jpg)、[关闭胶囊后的护眼轻提醒](evidence/eye-care/eye-gentle-detached.jpg)、[重启后的首页](evidence/eye-care/restarted-home.jpg)。测试日志与数据库核对摘要见 [事件证据](evidence/eye-care/session-evidence.txt)。

## 待外机／专项验证

- Windows 10 x64 尚未原生构建、安装或运行本版本；本机缺少 Windows 资源编译工具，Mac 构建成功不代表 Windows 通过。平台代码已实现，按手册在 Windows 构建和测试。
- Intel Mac 已产出 x86_64 包，macOS 15+、GPU、通知和多屏仍需 Intel 真机验证。
- 本轮未重复真实锁屏／睡眠操作；对应新待办语义已有模拟双时钟自动测试，不能代替真机。也未完成本轮混合 DPI、拔屏、全屏 Space、读屏、外部输入焦点、浅色模式和长时间稳定性测试。
- 提醒条布局支持能容纳五条（总高 224 逻辑像素）的正常桌面工作区；没有实现极小工作区内滚动。
- Mac 包为 ad-hoc 签名，未做 Developer ID 公证。签名状态沿用个人使用交付方式。

可直接执行的 20 组步骤见 [Windows 10 / Intel Mac 手动验证](EYE_CARE_REMINDERS_MANUAL_TESTS.md)。此前胶囊和多屏证据保留于 build 4 报告，不重复当成本版本全平台通过。

## 安装包

先从菜单栏退出旧版，再解压并替换 `.app`。本次只在功能分支交付，未推送 `release` 触发公开发布。

| 包 | SHA-256 |
| --- | --- |
| `dist/GuluTwist-0.1.0-build5-aarch64-apple-darwin.zip` | `5c80fdf6f6a89c2349d15d6f00e8937c147b2cc023ab23b57264108fd2a3236e` |
| `dist/GuluTwist-0.1.0-build5-x86_64-apple-darwin.zip` | `a8443e3ee4c400e45d28ac66d1a2eb6993c75eb465a536fa974e635e93847907` |
