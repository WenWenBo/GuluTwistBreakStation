# GitHub Releases 更新分发

实现日期：2026-10-14。运行代码位于 `crates/gulu-app/src/update/`，发布流程见 [打包说明](../packaging.md)。

客户端匿名读取固定公开仓库 `WenWenBo/GuluTwistBreakStation`，不携带 PAT。先读取最近 10 条 Release，选择首个非草稿、非 prerelease 正式版；读取本平台的 `releases.<channel>.json`，再下载清单对应的全量 `.nupkg`。生产版本每次必须包含三目标完整产物。清单读取失败、空源或缺失附件都报告错误，不当成“已是最新版本”。

```mermaid
flowchart LR
    BUILD[三个目标构建与校验] --> DRAFT[GitHub 草稿]
    DRAFT --> TEST[隔离安装和升级验收]
    TEST --> HUMAN[维护者手动发布]
    HUMAN --> CHECK[客户端匿名检查]
    CHECK --> CACHE[下载全量包与摘要校验]
    CACHE --> SAVE[用户点击重启并成功保存]
    SAVE --> APPLY[Velopack 替换并重启]
```

Rust SDK `GithubSource` 在 1.2.161 中未设置请求超时，并会静默跳过失败的 feed。本项目使用一个窄范围 `UpdateSource` 适配器修正这两点，仍由 Velopack 负责版本选择、下载完整性检查和替换。清单请求上限 30 秒，包下载上限 30 分钟；一次检查固定同一 Release 的附件 URL，不在下载中重新选择目标。只支持 HTTPS 官方仓库附件。

启动检查延迟 30–90 秒；成功后 12 小时加最多 30 分钟抖动；失败按 15 分钟、1 小时、3 小时、12 小时退避。手动检查一分钟冷却，自动检查至少间隔 15 分钟，并保存最近尝试时间。SDK 下载接口不暴露响应头，因此当前无法读取 Retry-After 或 GitHub reset 时间；状态保留实际错误，使用本地退避。2026-10-14 本机出口的 GitHub 匿名额度确实曾耗尽，不能以仓库网页可访问代替 API 和附件链路验收。

缓存描述保存在业务数据库独立的 `update_preferences` 键；缓存文件在 Velopack 包目录。恢复缓存与应用前重新检查原始 SHA256、大小、内嵌版本、身份和渠道，不实施签名密钥或清单公钥验证。安装前另检查可用空间和目标目录写权限。

草稿及 prerelease 不参与正式更新。首次上线不会让旧 `0.1.0` 自动获得更新器；用户手动换装一次后，下一正式版本才通过 OTA 更新。全量包允许跳过中间版本，不依赖历史差分链。没有镜像、服务器、账号或灰度服务。

如需复测草稿包，使用 [诊断方式](../validation/OTA_MANUAL_TESTS.md)；绝不能把维护者 GitHub 令牌放进客户端。生产发布的安装包和清单保持不变，修复采用更高版本。
