# 使用 GitHub Releases 分发更新

状态：建议接入方案，尚未实施。整理于 2026-10-14。返回 [OTA 文档入口](README.md)。

本轮已选择直接连接 GitHub、全量更新、自动构建草稿后手动发布，不做灰度或额外的更新清单签名验证，完整范围见 [已确认设计](../plans/2026-10-14-github-ota-design.md)。下文的自定义灰度仅保留为扩展参考。

## 是否需要对象存储或 CDN

不需要。现有仓库的 GitHub Releases 可以托管首次安装包、全量更新包、差分包与版本清单。GitHub 提供文件下载，客户端使用 HTTPS 获取它们；本项目无需因此额外购买存储或部署 CDN。

需要区分文件托管与更新决策：GitHub 可以承担文件分发，自建服务可以只负责名单、比例和撤回，不转发大文件。公开 Release 附件可匿名访问；私有仓库要单独设计认证，不能把维护者的访问令牌硬编码到所有客户端。

## 两种接入方式

| 模式 | 检查版本 | 下载产物 | 是否需要自己的服务 |
| --- | --- | --- | --- |
| 直接连接 GitHub | Velopack `GithubSource` 读取仓库 Releases 和渠道清单 | GitHub Release 附件 | 普通自动更新不需要 |
| 自定义灰度 + GitHub | 自建 API 决定精确目标，自有 `UpdateSource` 适配 | 被选定 Release 的附件链接 | 需要轻量决策服务，不需要另建文件存储 |

直接连接模式通常配置仓库 URL。SDK 查询 Releases 中的 `releases.{channel}.json`，再定位清单中的 `.nupkg` 附件。它不是把一个任意 ZIP 地址传给安装函数就能更新。

自定义模式可以直接下载 GitHub 的具体附件链接，但仍需提供正确的更新包、可信清单和目标版本信息。官方 `GithubSource` 的完整公共 feed 不应在灰度判断后再次独立选择最新版，否则可能越过指定范围。

```mermaid
flowchart LR
    APP[客户端] --> CHECK[灰度决策 API]
    CHECK --> TARGET[明确的目标版本与签名清单]
    TARGET --> APP
    APP --> GH[GitHub Release 指定版本附件]
    GH --> STAGE[本地缓存与更新准备]
    STAGE --> APPLY[退出并更新]
```

## 当前打包流程要增加什么

当前 [发布工作流](../../.github/workflows/check.yml) 只在 `release` 分支 push 时构建 Mac ARM64、Mac Intel 和 Windows x64 ZIP，[发布脚本](../../scripts/publish-release.sh) 上传三个 ZIP 与 `SHA256SUMS.txt`。这些现有 ZIP 尚不是 Velopack 更新产物。

接入时扩展这条流程：

1. 应用加入 Rust 更新 SDK 与启动 hook，统一版本、应用身份和更新渠道。
2. CI 正常编译，再使用锁定版本的 `vpk` 打包。初期只生成全量更新；开启差分前取得上一版本完整包。
3. 按本轮条件保留 Mac 必要的临时签名及更新包完整性校验，不新增正式代码签名、公证或更新清单签名流程。
4. 每个平台/架构使用独立渠道；上传对应全量包、渠道清单和首次发行包；本轮不生成差分包。
5. 多个构建任务的附件汇总到同一个草稿 Release，CI 到草稿即停止；维护者验证三个目标的产物后手动公开，客户端随后可发现更新。

例如本轮单个渠道的附件包含以下种类；确切文件名由锁定版本的 `vpk` 生成，不应手工猜测并写死：

```text
首次发行包（Windows 安装版、Mac 应用包）
...-full.nupkg
releases.win-x64-stable.json
```

建议渠道为 `win-x64-stable`、`osx-arm64-stable`、`osx-x64-stable`。不要让两个 Mac 架构写同一个渠道清单。Linux 不属于本轮。

## 下载链接和灰度约束

- 使用绑定具体 Release 和文件的下载信息，目标已选定后避免使用随时变化的 `latest` 链接。
- 包和清单保持不可变；修复发新版本，不覆盖已发布版本的同名附件。
- 灰度适配器只向 SDK 提供目标完整包及必要差分链，并验证目标、渠道、架构、摘要与签名。
- 客户端“看不到更新”不意味着公开附件不可访问。若测试包内容需要保密，不能依赖公开 Release 加灰度名单实现访问隔离。
- GitHub 托管并不替代客户端的更新可信性校验。当前 SHA256SUMS 只提供完整性信息。

## 使用限制与验收

公开 GitHub API 有匿名访问限流，同一出口 IP 的多台设备可能共享配额。检查应限频、加入抖动，并处理离线和服务错误。客户端的实际网络必须能稳定访问 GitHub 及其下载重定向域名；源码托管可访问不等于大包下载体验已合格。

本次核对的 Rust 1.2.161 `GithubSource` 读取最近一页、最多 10 个 Release；版本清单/产物保留策略需要考虑旧客户端和所需差分是否仍能发现。过旧客户端应有可直接升级的全量路径，自定义服务可返回固定版本附件，避免依赖无限遍历历史 Release。

本轮至少验证两个真实发行版之间的全量升级、平台/架构隔离、下载失败、包完整性校验失败、草稿不可见及手动公开后的可见性、旧客户端迁移。GitHub Releases 无法让当前没有更新代码的旧安装自动获得 OTA，仍需首次手动迁移。差分回退和额外的签名验证不属于本轮验收。

## 参考资料

- [官方 GitHub Actions 发布示例](https://docs.velopack.io/distributing/github-actions)
- [GitHub 与其他自托管更新源](https://docs.velopack.io/distributing/self-hosting)
- [渠道与跨平台分发](https://docs.velopack.io/packaging/channels)
- [Rust 1.2.161 GithubSource 源码](https://docs.rs/crate/velopack/1.2.161/source/src/sources/github.rs)
