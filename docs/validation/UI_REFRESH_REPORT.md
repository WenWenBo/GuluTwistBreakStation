# 小咕视觉更新验证报告

> 后续 build 3 修复 Dock 旧缓存，当前下载包已更新。下文 build 2 的历史验证与校验和保留；最新包与验证见 [Dock 图标修复](DOCK_ICON_FIX.md) 和 `dist/SHA256SUMS.txt`。

日期：2026-10-13。代码 / 应用版本 0.1.0，macOS 构建号 2。环境：Apple M4，macOS 15.7.3，当前三屏。测试只使用 `dist/ui-refresh-data`、`dist/ui-refresh-formal` 等独立目录。

## 实现范围

- 原创 SVG 小咕应用图标、透明圆角、配套插画和同风格导航 / 类别图形。
- 11 档 PNG、九尺寸 ICO、Mac ICNS、菜单栏单色模板、Windows 托盘 RGBA、EXE / 窗口图标链路。
- 侧栏、紧凑首页、窄窗口单列、暂停菜单、轻提醒稍后菜单、固定设置栏、分秒输入与未保存保护。
- 记录空态、紧凑记录行、关于页、单一标题的休息遮罩。
- 原有提醒引擎与存储格式未改变。

## 问题定位与修复依据

1. **方形图标**：旧 1024 PNG 的 alpha 全部为 255。新图有 373,298 个全透明像素，占画布 35.6%，边界抗锯齿；不是给旧图强加一个 CSS 圆角。
2. **倒计时缺损**：旧实现继承默认字体度量，使用 Light 大字号，没有专用行高和安全空间。改为专用等宽数字字体、常规字重、明确行高与留白；本机 `00:05` 等实际窗口完整。尚未将旧字体的具体底层栅格化路径定位为唯一根因，修复依据是前后实际效果，不能推断所有外机字体都已验证。
3. **额外发现的遮罩越界**：日志记录 Mac 遮罩视口 1470×956，但原生窗口起点被系统放在 y=38；外屏相应为 y=25，导致底部越出屏幕。GPUI 的 `titlebar=None` 实际仍创建带 Titled 样式的窗口。改为真正 Borderless 后起点为 y=0，底部退出说明完整，Esc 仍有效。

## 已执行检查

| 项目 | 结果 / 证据 |
| --- | --- |
| 格式、workspace clippy `-D warnings` | 通过，`dist/ui-refresh-checks.log` |
| 自动化测试 | 37 项通过：25 核心、9 SQLite、3 新表单解析测试 |
| 分秒转换 | 5 秒、59 秒、1 分、1 分 1 秒、30 分边界正确；非法秒数 / 溢出拒绝 |
| Windows 原生适配类型检查 | 通过；不等于 Windows 应用构建或真机运行 |
| 本机首次引导 / 保存 | 已执行，确认前不调度，保存返回首页 |
| 默认 860×700 首页 | 四卡、状态及暂停完整可见 |
| 约 640×480 窄窗口 | 图标侧栏、单列内容、固定保存栏正常，无横向溢出 |
| 非法秒数与离开表单 | 填 60 秒就地报错；离开出现继续 / 放弃 / 保存；放弃恢复旧值 |
| 暂停菜单与恢复入口 | 四项菜单显示，暂停 15 分钟后变为恢复按钮 |
| 休息体验 | 三屏创建；自然结束；Esc、⌘W 退出；均没有生成活动记录 |
| 修复后遮罩 | 真正无边框，退出按钮与说明完整；共享截止点保持 |
| 空记录页 | 角色与说明正常，计时体验后仍 0 项 |
| 正式合并提醒 | 两次各 65 秒，均创建 3 个遮罩，自然结束后共 2 场 / 8 条 Timed，无重复 |
| 锁屏冻结间隔 | 14:59:53–15:09:19 期间四项剩余均保持 178963 ms；解锁不补弹 |
| Release 重启 / 清空确认 | 两场记录保留；选择保留记录不删除；危险按钮已改为白字深红底 |
| Finder 图标 | 新包的小图与大预览均为小咕，透明圆角正常 |
| Release 轻提醒 | 完成喝水→Manual；稍后菜单5/10/15齐全，颈肩选5分后显示稍后倒计时；跳过放风→Skipped；站立120秒未处理→Expired |
| 测试退出 | ⌘Q 正常退出，退出码 0；尚在稍后的颈肩记录为 Cancelled（应用退出） |
| 包的签名 / 架构 / 图标 | 见下方最终包检查及 `dist/SHA256SUMS.txt` |

截图位于 [ui-refresh](../design/references/ui-refresh/)：`icon-concepts.png`、`home.png`、`formal-overlay.png`、`home-narrow.png`、`settings-narrow.png`、`records-empty.png`、`records.png`、`finder-icon.png`、`clear-confirmation.png`、`gentle.png`、`snooze-menu.png`、`about.png`、`home-release.png`。截图是实际 GPUI 窗口，不是网页仿稿；探索图板例外。

## 最终包与剩余实测

本轮 Apple Silicon / Intel Release 包均已由现有脚本构建成功，`codesign --verify --deep --strict` 通过；二进制分别确认为 arm64 / x86_64，包内均含新版 ICNS，构建号 2。Windows 资源嵌入和窗口加载代码已实现；当前 Mac 缺 Windows SDK，Windows EXE / 安装包仍需在 Windows 原生生成。

用户解锁后已完成正式四项提醒回归。`dist/ui-refresh-formal.log` 记录 15:12:19→15:13:24 与 15:18:31→15:19:36 两次正式合并休息；SQLite 各 4 条 Timed，分别共享一个 session。正式遮罩截图 `00:07` 的数字、冒号和底部退出区完整。15:21:34 体验后的 ⌘W 在约 0.52 秒后触发 Interrupt，未增加活动记录。最终 Release 另用 `dist/ui-refresh-release.log` 留存证据；重启后 8 条正式记录保留。

最后实看发现清空确认按钮沿用组件库的红色文字，与新深红底对比不足。已将普通 / hover / 按下状态配为深红底白字，再次运行全部检查并重建两套发布包；`clear-confirmation.png` 为修复后 Release 截图。

本轮验证限制：新无边框遮罩在界面工具中只返回窗口节点，没有按钮 AX 子节点；外屏坐标点击返回 `windowNotFoundAtPosition`，因此最终版的逐屏鼠标点击不能标为通过。Esc / ⌘W 有成功中断证据，退出按钮的可见性有截图证据。Dock 工具读取超时，仅 Finder 和包内资源完成实看 / 核验。清空确认目前是正文内的确认卡，保留按钮在前；提案中的自动聚焦取消尚未实现，列入后续键盘与读屏完善项。

仍需执行：Windows 10 / Intel 真机、混合 DPI 全矩阵、逐屏退出 / 热插拔 / Space、Dock 对比与菜单栏亮暗、完整读屏、8 小时稳定性、真实睡眠。既有双时钟单测通过与之前的锁屏实测仍有效；真实睡眠之前仅取得锁屏证据，不改写为通过。操作步骤见 [外机视觉专项](UI_REFRESH_MANUAL_TESTS.md) 和 [完整外机验证](MANUAL_TEST_GUIDE.md)。

最终数据库核对：8 条 Timed 分属两个 session（每场 4 项），另有 Manual / Skipped / Expired / Cancelled 各 1 条。预览没有新增记录。所有测试实例已正常退出，未将 5 分钟测试配置写入日常用户数据。

## 包校验和

| 包 | SHA256 |
| --- | --- |
| Apple Silicon | `b1df0b83a64a6d39c9ba5d5ab473381b636861567053f0ca8f887ded2dcee0e3` |
| Intel | `a7464f3bd7cf0baa27af4ed87b1ef2c5e8800f04d19c0190380494463053efac` |

包路径分别为 `dist/GuluTwist-0.1.0-aarch64-apple-darwin.zip` 与 `dist/GuluTwist-0.1.0-x86_64-apple-darwin.zip`。构建是本机交叉构建；Intel 的真实运行及 Windows 资源编译不在上述成功声明内。
