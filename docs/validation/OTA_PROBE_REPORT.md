# Velopack 可行性与本机升级实验

日期：2026-10-14。代码基线 `feff71b`，测试对象为其上本次 OTA 工作区改动；测试产物没有上传或发布。主机 macOS **15.7.3 / Apple Silicon**，Rust **1.97.1**，.NET SDK **10.0.402**，Rust Velopack 与 vpk 均为 **1.2.161**。

## 工具与产物

已实际执行 `vpk pack --help`、`vpk pack -H`，Mac 使用 `--runtime osx-arm64/osx-x64`、独立 stable channel、`--packDir <app>`、`--mainExe gulu-twist`、`--bundleId app.gulutwist.breakstation`、`--signAppIdentity -`、`--noInst`、`--delta None`。vpk 在注入更新器后签名，生成首次 ZIP 和 full.nupkg。最终 ZIP 解压后通过 `codesign --verify --deep --strict` 和图标检查。

Windows `--aumid` 与 `--instLocation PerUser` 已与 1.2.161 的 `WindowsPackCommand.cs` / `InstallLocation.cs` 核对；原生 vpk 执行仍待 Windows CI。Windows nupkg 使用根目录 nuspec，安装时才形成 `current/sq.version`；产物校验已按这一差异实现，不能要求 Windows 更新包预先含 sq.version。

| 目标 | 本次产物 | 结果 |
| --- | --- | --- |
| ARM64 A | `dist/ota/aarch64-apple-darwin/` 的 0.1.1 debug ZIP/full/feed | 编译、打包、归档、身份、SHA256、最终签名、图标通过 |
| Intel A | `dist/ota/x86_64-apple-darwin/` 的 0.1.1 debug ZIP/full/feed | Apple Silicon 主机交叉编译与同类产物校验通过，未在 Intel 机器运行 |
| ARM64 B | `dist/ota-probe/source/dist/ota/aarch64-apple-darwin/` 的 0.1.2 debug 包 | 隔离源码副本生成，只供本地实验 |
| Windows | 未生成 Setup | macOS 上全应用交叉检查在 GPUI 的资源编译阶段因缺少 llvm-rc 失败；适配检查桩通过，不代表 Windows 应用通过 |

诊断安装位于 `dist/ota-probe/installed/GuluTwist Break Station.app`，业务数据为 `dist/ota-probe/data/station.sqlite3`，包缓存为该数据目录下 `velopack-packages`。文件源指向上述 B 的目录。没有修改 `/Applications` 的日常安装或其业务数据库。SDK 辅助器仍写入系统 Velopack 日志路径，这是日志路径差异，不影响业务/缓存隔离。

## 实际操作与结果

1. A 首次运行确认引导，自动下载 B；主窗口隐藏时独立弹窗出现，版本和说明正确。
2. 点击稍后后主窗口反复打开不重弹；创建一条喝水测试记录，并关闭自动检查。
3. 普通退出后安装目录仍为 0.1.1；移走文件源清单、保留目录后重新启动 A，缓存恢复并再次提示，未自动安装。
4. 显式点击立即重启：更新器日志显示 `WaitPid` 指向旧进程，旧进程结束后才提取包、替换 bundle，再启动新 app。磁盘和关于页均显示 0.1.2，记录仍为 1，automatic=false 保留，ready 清理。
5. 新版首次访问关于页发现 debug 构建可访问性节点重复导致崩溃。用相同操作复现，栈指向 GPUI `A11yNodeBuilder::can_push`，确定是多个空动态 caption 共用文字 ID。改为固定独立 ID 后重新打包 A/B，重复升级；新版关于页稳定可用，文本可被可访问性树读取。
6. 重装隔离 A，缓存同名旧 B 与新 feed 摘要不一致时，客户端删除旧缓存并重新下载；比较缓存 SHA256 与新清单，匹配。
7. 在测试 SQLite 中加入拒绝 `preferences` 写入的 trigger，再点击更新。页面显示“未重启：本地数据库错误：OTA test: write failed”，进程仍为 A，记录不变，未启动更新辅助器。删除 trigger 后，无网络清单仍能显式更新成功。
8. 再次启动隔离 A 的就绪缓存，保留更新弹窗，通过应用菜单打开主窗，修改喝水间隔 30→35，Cmd-W 隐藏主窗，再在更新弹窗点立即重启。设置页重新出现并提供三个选择；选保存并离开后升级，新版首页显示 35:00，数据库中间隔为 35，原记录仍为 1。
9. 结束测试时关闭隔离测试实例，保留 `dist/ota-probe/` 供复查。

## 网络观察与未执行项

匿名仓库 Releases API 在本机返回 **403**，响应头为 `X-RateLimit-Remaining: 0`。不引入账号或把令牌放入客户端；客户端使用本地退避。当前没有本次新版本的公开 Release，也没有上传测试 Release，所以草稿对匿名客户端不可见、手动发布后的真实附件下载/重定向、真实 GitHub A→B **未实测**。

Windows 10/11 原生安装/升级/卸载、Intel 原生升级、下载 quarantine/App Translocation、系统安全提示、不同安装目录权限、多屏/锁屏后的 OTA 交互仍按外机手册补验。本地 debug 包与源目录测试不能替代这些项目。

包装时曾发现目录创建早于图标生成，跨 ZIP 时间粒度边界可触发原有“stale bundle modification time”检查；恢复打包前刷新 bundle/Contents mtime 后通过，最终签名仍由 vpk 完成。没有放宽图标校验，也没有在 full.nupkg 生成后改动 bundle。
