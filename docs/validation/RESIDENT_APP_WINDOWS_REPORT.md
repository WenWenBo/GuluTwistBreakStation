# Windows 托盘常驻本机验证

日期：2026-10-14（北京时间）。验证分支：`codex/bugfix`；基于提交 `403f642` 加上本报告随附的 Windows 修复构建以下便携包。结论：Windows 11 原生构建和主要应用生命周期通过，发现并修复重复启动错误；托盘系统区域交互等专项未完成，不能视为完整 Windows 验收。

## 环境与产物

- Windows 11 专业版 x64，10.0.26200；Intel Core i5-8500，6 逻辑核。
- Rust 1.97.1，`x86_64-pc-windows-msvc`，Visual Studio C++ Build Tools 位于 `D:\BuildTools`。
- 本轮正式休息实际创建 1 个遮罩；未采集全部显示器及 DPI 信息，不推断多屏或缩放兼容性。
- 所有运行使用独立的 `GULU_DATA_DIR`，证据根目录为 `dist/resident-windows-20261014/`，不使用日常数据库。
- 测试的是解压便携包 `portable/gulu-twist.exe`。初期开发包仅用于问题复现；下表除特别说明均为修复后的 Release 包。

| 产物 | SHA-256 |
| --- | --- |
| `dist/GuluTwist-0.1.0-Windows-x64-portable.zip` | `ac5052790956c87119247dd9d56950f287b63bfaf7a642b7b5846d3674e46e6a` |
| 解压后的 `gulu-twist.exe` | `e6e564ac6e7ff4492b3bfd546dac29f3c986350103f81df02b78a8d7ec098862` |

## 问题与修复

1. **重复启动弹错误，无法唤回旧实例。** 第二次运行同一数据目录出现原生提示“另一个程序已锁定文件的一部分，进程无法访问（os error 33）”。`fs2` 在 Windows 返回 `ERROR_LOCK_VIOLATION`，原代码只判断 `ErrorKind::WouldBlock`。改为与 `fs2::lock_contended_error()` 的原生错误码比较。增加真实文件锁竞争／释放测试。Release 复验连续三次启动，第二进程全部返回 0，原进程与窗口保持不变，隐藏和最小化后均可恢复。
2. **Windows 专属 Clippy 失败。** 6 处可合并的条件判断和 1 处整数资源 ID 转指针。合并条件；图标仍使用资源 ID 1，通过 `without_provenance(1)` 明确表达 `MAKEINTRESOURCEW(1)`。不能采用 lint 建议的 `dangling::<u16>()`，它的地址不是资源 ID 1。

初次测试启动器使用 `Start-Process -WindowStyle Hidden`，导致首次主窗被 Windows 启动参数隐藏；此为测试启动方式问题，不记为产品缺陷。之后交互实例以 Normal 启动。该首次隐藏实例（PID 7392）被定点终止，其余实际交互测试通过应用退出；最终没有残留测试进程。

## 自动检查

| 命令 | 结果 | 证据 |
| --- | --- | --- |
| `cargo test --locked --workspace -j 4` | 修复前 56 项通过；修复后 57 项通过（应用 13、核心 31、存储 13） | `tests.log`、`tests-fixed.log` |
| `cargo fmt --all --check` | 通过 | 最终终端检查 |
| `cargo clippy --locked --workspace --all-targets -j 4 -- -D warnings` | 初次失败，修复后通过 | `clippy.log`、`clippy-fixed.log` |
| `./scripts/package-windows.ps1 -SkipInstaller` | 原生 Release 编译、ZIP 打包与解压运行通过 | `package-fixed.log` |
| `git diff --check` | 通过 | 最终终端检查 |

本机 Codex 终端 PATH 没包含已安装的 Rust；在命令进程内添加 `$env:USERPROFILE\.cargo\bin` 后执行，未修改系统 PATH。

## 原生运行结果

编号对应 [常驻手工回归](RESIDENT_APP_MANUAL_TESTS.md)。下表仅确认列出的子场景，不表示整个编号的全部边界通过。日志和文本路径相对于证据根目录。

| 场景 | 实际结果 | 证据 |
| --- | --- | --- |
| C-01 首次引导 | 最终包新目录显示引导；保存成功后主窗隐藏，托盘可用；再次启动可恢复 | `release-onboarding.log`、`09-release-onboarding.txt` |
| C-02 安静启动 | 胶囊关闭和开启两种设置均 `main_exists=false`；开启时 `floating_visible=true`，关闭时 false | `release.log`、`floating-cold.log` |
| C-04 关闭／重开 | 单次关闭后 `main_visible=false`、进程存活；重新运行复用同一主窗 | `release.log`、`01-release-reopen.txt` |
| C-05 草稿保护 | 喝水间隔 30 改 31 未保存，关闭／重开与最小化／还原后仍为 31；离开设置显示继续编辑／放弃／保存；退出后数据库仍为 30 | `02-draft-restored.txt`、`03-minimize-restored.txt`、`04-unsaved-navigation.txt`、`interactive/station.sqlite3` |
| C-06 最小化还原 | 重复启动恢复原窗口及设置草稿，实际窗口画面可编辑 | `03-minimize-restored.txt`、`release.log` |
| C-07 纯后台到期 | 无主窗、无胶囊，全天配置喝水轻提醒＋护眼全屏，间隔 5 分钟；15:24:12 启动，15:29:12 产生提醒，15:29:13 开始 30 秒休息，15:29:43 自动撤罩并写一条 Timed | `background.log`、`background-records.json` |
| C-07 轻提醒结算 | 休息结束后重新打开主窗，喝水仍待完成；点击已完成后仅新增一条 Manual | `08-background-after-rest.txt`、`background.log`、`background-records.json` |
| C-10 单实例 | 连续三次启动相同目录，第二进程均返回 0，保留原 PID 6380 和原主窗；正常退出后可冷启动 | `release.log`、本轮终端记录 |
| C-11 托盘首次创建失败 | `tray-once` 保留兜底窗；关闭不会隐藏最后入口；点击重试后注册成功、错误入口移除、主窗保留 | `fault-once.log`、`05-tray-fallback.txt` |
| C-11 持续注册失败 | `tray-registration` 重试后仍失败，不假报成功；兜底退出按钮正常结束进程 | `fault-registration.log`、`07-registration-still-failed.txt` |
| C-11 双入口失败 | `tray,main-window` 显示原生错误提示；确认后发送 Quit 并退出，没有崩溃 | `fault-both.log`、`06-both-failure.txt` |
| C-12 完整退出 | Ctrl+Q 正常结束主窗／胶囊实例；第二次正式护眼休息期间 Ctrl+Q 撤罩，写一条 Interrupted，进程消失 | `floating-cold.log`、`background.log`、`background-records.json` |

后台数据库最终有 3 个不同 instance_id：第一轮护眼 Timed、喝水 Manual、第二轮护眼因应用退出 Interrupted，未发现重复结算。第二轮休息恰好在退出前开始，不能将它误计为第一轮重复记录。

窗口截图和输入由 Windows Computer Use 操作；持久化证据保存为 UI 可访问性文本、应用日志和隔离数据库。工具不能稳定列举工具窗口；部分截图捕获到遮挡目标的其他窗口，因此不把这些截图当作目标窗口的视觉证据。正式休息画面与 `overlay_count=1`、数据库结算共同确认休息运行；轻提醒独立条的视觉和系统通知点击未验收。

## 未完成的专项

- W-01／W-02：主窗隐藏／恢复行为已验证；任务栏、Alt+Tab 的实际列表及辅助窗口创建瞬间是否闪现未验证。
- W-03：工具没有提供可定位的托盘／任务栏系统窗口，未实测托盘左键、右键、键盘导航、展开区操作。`tray_available=true` 只证明注册检查成功。
- W-04：未在日常会话重启 Explorer，也未测试 Explorer 尚未就绪时启动或锁屏中的故障恢复。
- W-05：100%／150%／200% DPI、多屏混合缩放、主屏切换及热插拔未测试。
- W-06：便携版已运行；Inno Setup 不在预期安装位置，未生成安装包，未验收安装、开始菜单、卸载、普通用户权限矩阵或通知身份。
- Windows 10、真实锁屏／睡眠、休息期间延后打开、按键不能逃离遮罩、20 次生命周期循环、保存失败／保存中退出、全部事项关闭／暂停／下班冷启动尚未补齐。
- 未执行性能基线对比或 8 小时长稳，不宣称 CPU／内存降低或性能达标；本轮未重新验证 macOS。

上述验证与包校验和对应本轮测试构建，不自动覆盖之后的合并或重新构建。便携 ZIP 仅含 exe 与图标，不含测试数据；未发布 Release。
