# GitHub Actions 自动打包

工作流配置位于 `.github/workflows/check.yml`。只有 `release` 分支收到 push 时才运行；每次都构建以下三个目标。`main`、功能分支、标签推送和 PR 事件均不触发这个工作流，也没有手动触发入口。

| 下载产物 | 构建机器 | 内容 |
| --- | --- | --- |
| `desktop-macos-arm64` | `macos-15` | Apple Silicon `.app` 的 ZIP，macOS 15+ |
| `desktop-macos-intel` | `macos-15-intel` | Intel `.app` 的 ZIP，macOS 15+ |
| `desktop-windows-x64` | `windows-2022` | Windows x64 便携 ZIP，包含 EXE 和图标 |

每个目标先执行格式检查、workspace 测试和编译检查，然后调用现有平台打包脚本。Mac 两个架构分别构建，不是 Universal 包；每份 Artifact 只收录一个带 build 号的 ZIP，避免重复上传兼容旧名称的同一份包。Windows 使用 `-SkipInstaller`，不生成 Setup 安装程序。一个目标失败不会取消其他目标。

三个目标全部成功后，工作流自动发布 GitHub Release，附件包含三份 ZIP 和 `SHA256SUMS.txt`。任何构建失败都会阻止发布。发布步骤使用 GitHub 自带的 `GITHUB_TOKEN`，仅该任务拥有 `contents: write` 权限，无需另配个人令牌。

标签格式为 `v<应用版本>-build.<工作流运行序号>`，例如 `v0.1.0-build.3`。每次 push 使用新的序号，即使应用版本没有变化也不会覆盖旧包；此序号与 Mac 应用内的 build 号独立。标签指向本次构建的准确提交。发布脚本先创建草稿，所有附件上传成功后才公开；上传失败可重跑任务恢复。同一次运行重跑使用相同标签，已经公开的 Release 不覆盖。

## 发布一次构建

在功能分支开发，准备好后合并到 `main`。需要发包时，在工作区干净的情况下执行：

```bash
git fetch origin
git switch release
git merge --ff-only origin/release
git merge origin/main
git push origin release
git switch main
```

首次在另一份克隆中使用 `release` 时，可先执行 `git switch --track origin/release`。如果没有新提交，push 不会产生新构建。若合并有冲突，先解决并提交，再推送。

触发规则读取对应分支中的 workflow 文件。新功能分支应从包含本配置的最新 `main` 创建；已存在且保留旧 `on: [push, pull_request, workflow_dispatch]` 配置的分支，需要同步这项变更，否则旧规则仍可能触发构建。新增其他 workflow 时也要保持相同的触发约束。

## 下载与验证

打开 [Releases 页面](https://github.com/WenWenBo/GuluTwistBreakStation/releases)，在对应版本的 **Assets** 下载平台 ZIP；公开仓库的 Release 附件可以直接分享给使用者。Release 附件不受 Actions Artifact 保留期影响，除非手动删除或受仓库管理策略影响，会持续保留。

也可以打开 [Actions 页面](https://github.com/WenWenBo/GuluTwistBreakStation/actions)，进入对应 `release` 提交的 `Desktop builds` 运行记录，在底部 **Artifacts** 下载所需平台包。Artifact 保存期限遵循仓库设置，通常需要登录 GitHub。发布任务的摘要提供 Release 链接。ZIP 不提交到 Git 历史；Git 中仅新增指向源码提交的发布标签。

Mac 使用现有脚本的 ad-hoc 签名，尚未配置开发者签名和公证。Windows 构建在 Windows Server 2022 上执行；构建通过不替代 Windows 10 的实际运行验证。跨平台通知、多屏遮罩和睡眠恢复仍按 `docs/validation/MANUAL_TEST_GUIDE.md` 验证。
