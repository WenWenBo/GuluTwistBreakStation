# info-im 更新方案对比

检查日期：2026-10-14。参考应用源码基线：`97ba1c7679a1632e33de5f448330945a8b19f12f`。本页来自本地源码静态检查，未执行更新，未检查其独立 OTA 服务端实现或线上配置。返回 [OTA 文档入口](README.md)。

## 结论

`info-im` 使用 Electron，自研更新编排结合 ASAR 更新、NSIS 和 electron-updater；未使用 Velopack。它与本项目的检查、下载、准备、重启更新流程相似，但额外提供了 Electron 特有的业务代码包切换能力。

它的“热更”是重启生效的 whole-ASAR 更新；本次检查到的主路径不生成 ASAR 二进制差分。Rust + GPUI 的逻辑编译进原生程序，不能直接复用 ASAR 加载器。

## 两条更新路径

应用分为安装基座和业务包：基座包含 Electron 运行时、启动层与原生能力；ASAR 包包含 JavaScript 主进程逻辑、渲染界面、资源与部分纯 JS 依赖。

| 条件 | 路径 |
| --- | --- |
| `shellVersion` 不一致，或 `forceShellUpdate` 开启 | 完整应用更新 |
| 基座兼容且 `asarVersion` 更高，存在 ASAR 产物 | ASAR 业务包更新 |
| 缺少新协议版本字段或 ASAR 产物 | 回退到完整应用更新 |
| 配置选择旧 OTA | 兼容旧更新任务 |

`shellVersion` 按是否相等判断兼容性；`asarVersion` 按是否更高判断业务包更新。首次加载、终版、提示与取消等策略由客户端更新服务协调。

## ASAR 如何更新

打包脚本把完整 `app.asar` 复制为 `update.asar`，压缩为 `appUpdate.zip`。不重新下载 Electron 基座可减少体积，但不是只下载发生变化的字节。

```text
appUpdate.zip → 下载到用户 update 目录 → 校验 SHA512
    → 解压 update.asar
    → 保存为用户 resources/app-版本号.asar
    → 记录选定版本
    → 重启时由安装 shell.js / dynamicLoad.js 选择新入口
```

普通 ASAR 更新向用户目录写入版本化文件，安装目录里的原始 ASAR 仍保留。启动加载器验证兼容条件并选择新包中的主进程入口；若没有合适的新包则使用安装资源。它更新的不仅是界面，也可包含主进程 JS 业务代码。

原生模块、宿主可执行文件、Electron ABI、签名权限和安装布局的变化不能按普通 ASAR 资源更新处理。参考应用为特定原生能力做了复用与降级设计，不能因此推导为任意 native 依赖都可热更。

## 完整应用如何更新

| 平台 | 源码中看到的机制 |
| --- | --- |
| Windows | 下载并校验 NSIS EXE，以 `--updated /S --force-run` 启动独立安装器进程；确认子进程启动后退出旧应用 |
| macOS | `MacUpdater` 读取选定 feed、准备更新，确认后 `quitAndInstall()`；项目文档说明底层由 Squirrel.Mac/ShipIt 替换应用 |
| Linux | 存在 AppImage/deb 配置和 Linux 清单路径，但当前主路径的完整安装执行分支未实现与双端等价的行为，不能据此宣称完整 OTA 已验证 |

这里的安装器或 ShipIt 与 Velopack 辅助进程承担相似职责：主应用退出后仍有进程执行安装。ASAR 更新则主要依赖版本化资源和下一次启动切换，不需要每次运行完整安装器。

## 灰度与失败处理

客户端先读取 `grayMember.json`，匹配对应平台的登录用户 ID 或部门 ID，命中后切换到指定版本目录读取 `latest*.yml`。本次检查到的规则是客户端名单匹配，未看到这条路径实现百分比分桶或应用前在线授权。独立服务端可能存在额外逻辑，本页不作推断。

当前默认自动检查周期为 30 分钟。ASAR 下载准备后可以立即重启，也可以记录新版本供后续启动使用，因此“稍后更新”不等于永远拒绝该版本。

项目有加载成功标记与若干回退选择，但其领域文档明确说明：没有通用的新版崩溃自动隔离与恢复旧版机制。撤回分发不自动降级已经激活的 ASAR；一般需要将验证过的内容重新发布为更高 `asarVersion`。这些边界也适用于我们对 OTA 回退能力的规划。

## 可借鉴与不可直接复用的部分

| 项目 | info-im | 本项目建议 |
| --- | --- | --- |
| 客户端职责 | 检查、下载、通知、退出编排 | 沿用职责拆分，接入 GPUI 事件协调器 |
| 日常更新 | 完整 ASAR 业务包，保留 Electron 基座 | 原生应用更新，可选差分下载 |
| 执行工具 | 自研加载器、NSIS、MacUpdater | Velopack SDK 与辅助进程 |
| 灰度身份 | 登录用户与部门 | 安装 ID，后续按需扩展账号 |
| 灰度选择 | 已检查路径由客户端匹配名单 | 自建 API 选择目标、稳定比例分桶 |
| 文件托管 | 配置使用自有 HTTP 更新源 | GitHub Releases 足够起步 |
| 业务回退 | 不具备通用 crash rollback | 明确单独设计和验收，不默认承诺 |

建议借鉴版本兼容、指定目标、下载缓存、单实例和退出协作，以及基座/数据兼容性意识。ASAR 机制不直接迁入本项目；无需为了更新而新增 JavaScript 运行时。完整原生应用更新也可以实现无需用户反复安装的体验。

## 源码证据索引

以下路径均相对参考应用仓库，并非本仓库文件。保留路径与提交基线用于复查，不包含内部服务地址、账号或发布凭据。

| 参考应用路径 | 检查内容 |
| --- | --- |
| `app/main/update/update-info.js` | 灰度名单、更新清单、版本判断 |
| `app/main/update/update-srevice.js` | 更新任务选择、30 分钟检查循环 |
| `release/afterPack.js` | 完整 ASAR 压缩为 appUpdate.zip |
| `release/incrementUpdate.js` | ASAR 摘要与版本写入更新元数据 |
| `app/main/update/UpdateTasks/AppAsarUpdateTask.js` | 下载、校验、版本化资源、重启 |
| `app/main/shell.js`、`app/main/dynamicLoad.js` | 安装启动层选择业务入口 |
| `app/main/update/UpdateTasks/FullUpdateTask.js` | Windows 独立安装器启动与其他平台分支 |
| `app/main/update/UpdateTasks/mac-full-updater.js` | MacUpdater 接入 |
| `docs/domain/ota-update.md` | 两类更新契约、失败回退实际边界 |
