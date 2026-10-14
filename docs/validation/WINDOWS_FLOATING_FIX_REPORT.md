# Windows 悬浮窗修复验证

日期：2026-10-14。环境：Windows 11 x64（10.0.26200）。

## 工具链

- Rust / Cargo 1.97.1，含 rustfmt、Clippy；与 `rust-toolchain.toml` 一致。
- Visual Studio 2022 Build Tools 17.14.41：`D:\BuildTools`。
- Windows SDK 10.0.26100.0。安装完成，无需重启。

## 修复与运行中发现的问题

1. 锁定的 GPUI Windows 后端未实现 `start_window_move`。Windows 改为释放鼠标捕获并投递 `WM_SYSCOMMAND / SC_MOVE | HTCAPTION`，让原生移动循环在 GPUI 回调返回后运行；其他平台保留原接口。
2. Windows 原生定位会同步发送 `WM_SIZE`，其 GPUI 回调此时无法重新借用 App，造成布局尺寸过期。定位完成后通过 `Window::defer` 同步尺寸。
3. 首次激活隐藏菜单会让 GPUI 重放旧的窗口位置和尺寸。Windows 菜单改用原生显示和激活，保留适配层设置的尺寸；macOS 仍调用原有 `activate_window`。
4. 首轮实测捕获到直接调用 `bounds_changed` 会重入已借用的 `FloatingView` 并在启动时 panic。改成延迟同步后重新构建、启动和跑完整测试，未再出现该 panic。

## 已执行

在 Visual C++ x64 编译环境运行：

| 检查 | 结果 |
| --- | --- |
| `cargo fmt --all --check` | 通过 |
| `cargo test --locked --workspace -j 4` | 54 项通过：应用 10、调度 31、存储 13 |
| `cargo check --locked --workspace -j 4` | 通过 |
| `cargo build --locked -p gulu-app -j 4` | 通过，生成 `target/debug/gulu-twist.exe` |
| 独立数据目录启动 | 通过：进程持续运行、悬浮窗句柄存在、初始位置写入测试数据库、诊断日志无 panic |
| `git diff --check` | 通过 |

新增 Windows 回归测试使用真实隐藏 HWND，验证释放鼠标捕获并把原生移动命令加入消息队列。它不替代实际鼠标拖动验收。

独立测试目录为 `.local-test/windows-floating`。本次未生成安装包或发布版本。

## 第二轮复验（2026-10-14）

- 重新执行格式检查、完整测试、`cargo check` 和开发版构建，全部通过，仍为 54 项测试。
- 本轮桌面自动化成功识别主窗口并取得界面截图，主窗口内容正常；悬浮球和菜单仍未作为可操作窗口返回。
- 用户在本机操作本次测试实例后明确确认：**“拖动和菜单都正常”**。这是人工交互验收结果，不是自动截图结论。
- 诊断日志在 12:00:13 和 12:00:15（北京时间）分别记录 `FloatingMenu` 和 `CloseFloatingMenu`，无 panic。
- 使用主窗口 `Ctrl+Q` 正常退出，日志记录 `Quit`。退出前保存的位置为 `x=1696, y=942`；使用相同独立目录重新启动后，数据库中的开关、显示器标识和位置保持一致，进程正常响应且无 panic。未独立测量重启后的屏幕坐标。

## 尚未通过或未执行

- 严格 Clippy（`cargo clippy --locked --workspace --all-targets -j 4 -- -D warnings`）被 Windows 适配层原有 7 处警告阻塞：6 处 `collapsible_if`、1 处 `manual_dangling_ptr`。后者是图标加载的整数资源 ID 写法，不应直接套用 dangling pointer 建议。
- 悬浮球和菜单无法通过桌面自动化工具直接操作、截图；拖动和菜单边距由用户本机确认正常。尚未逐项验证四角定位、点击与拖动区分、混合 DPI 拖动及重启后的屏幕坐标。
- 未在 macOS、Windows 10 或混合 DPI 多屏实机回归；macOS 定位函数仅增加未使用的上下文参数，激活封装仍调用原接口，拖动分支不变。

以上范围不能解读为所有平台或所有视觉场景均已验证。
