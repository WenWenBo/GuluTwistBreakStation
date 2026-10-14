# 打包与草稿发布

版本来自根 `Cargo.toml` 的 `workspace.package.version`，首个 OTA 版本为 `0.1.1`。三个目标使用同一产品版本、独立稳定渠道：

| Rust 目标 | Velopack RID / channel | 首次安装 |
| --- | --- | --- |
| aarch64-apple-darwin | osx-arm64 / osx-arm64-stable | Portable.zip 中的 `.app` |
| x86_64-apple-darwin | osx-x64 / osx-x64-stable | Portable.zip 中的 `.app` |
| x86_64-pc-windows-msvc | win-x64 / win-x64-stable | Setup.exe 用户安装版 |

## 本地构建

安装 Rust（仓库锁定 1.97.1）、Python 3.11+ 和 .NET SDK **10.0.402**。Mac 另需 Xcode Command Line Tools；Windows 使用 MSVC 工具链。`.config/dotnet-tools.json` 固定 vpk **1.2.161**，与 Rust SDK 一致。.NET 只用于构建，用户运行 Rust 应用无需 .NET。

```bash
./scripts/check.sh
scripts/package-macos.sh aarch64-apple-darwin
scripts/package-macos.sh x86_64-apple-darwin
```

Windows PowerShell：

```powershell
./scripts/package-windows.ps1
```

Mac 可用第二参数 `debug` 做诊断；正式发布默认 release。产物位于 `dist/ota/<Rust 目标>/`，含首次包、`*-full.nupkg`、`releases.<channel>.json`、源码提交元数据及 vpk 辅助清单。CI 发布脚本只选择经校验的首次包、全量包、渠道清单、build 元数据和汇总校验文件。旧 Inno 脚本保留为历史资料，不参与 OTA 构建。

Mac 在 vpk 加入 UpdateMac 和 sq.version 后做 ad-hoc 临时签名，随后生成两个归档；脚本解压最终 ZIP 再验证签名和图标。Windows 显式保留 `app.gulutwist.breakstation` AUMID，固定当前用户安装，不要求管理员权限。两端没有正式平台证书，Mac 未公证。

## 发布步骤

1. 提升 Cargo 产品版本、更新锁文件并添加 `docs/releases/<version>.md`；同一版本不能用于不同代码或不同构建内容。
2. 合并经过验证的代码到 `release` 分支并推送。只有这条分支的 push 触发发布工作流；PR 验证没有发布权限。
3. 三个目标全部成功后，CI 校验平台、架构、版本、内嵌清单、摘要、源码 SHA 和完整性，创建标签名为 `v<version>` 的 **草稿 Release**。
4. 下载草稿附件，在隔离数据和安装位置执行 [实机验收](validation/OTA_MANUAL_TESTS.md)。草稿对匿名客户端不可见，使用受限的诊断文件源验证同一份包。
5. 验收通过后，由维护者在 GitHub 点击发布。自动化不会执行公开操作。首次公开 `0.1.1` 后旧用户仍需手动安装一次。

发布脚本：`scripts/publish-release.sh dist/release`；CI 提供 `GH_TOKEN`、`GH_REPO`、`GITHUB_SHA`。只有发布任务具有 `contents: write` 权限。客户端不携带这些令牌。

上传失败时，只能用原始三份 Actions artifact 重跑发布任务；发布清单把每个文件的大小与 SHA256 绑定到该草稿。同一草稿不同 SHA、不同字节、未知附件、已有同名异提交标签均拒绝覆盖。已公开版本不可修改；修复需要提升版本重新走草稿流程。GitHub 查询失败不会当成“版本不存在”。

等上传工作流完整结束后再手动发布，避免人工公开与附件上传并发。

构建成功不代表原生安装与升级通过。当前实测范围、外机缺口和 GitHub 限流情况见 [OTA 验证报告](validation/OTA_REPORT.md)。
