#!/usr/bin/env python3
"""Create/resume one immutable-identity draft. Never publish a release."""
import importlib.util
import json
import os
import pathlib
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("validator", ROOT/"scripts/validate-ota-artifacts.py")
validator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(validator)

def gh(*args, optional_404=False):
    result = subprocess.run(["gh", *args], cwd=ROOT, text=True, capture_output=True)
    if result.returncode:
        if optional_404 and "(HTTP 404)" in result.stderr: return None
        raise RuntimeError(result.stderr or result.stdout or "GitHub request failed")
    return result.stdout

def main(directory):
    repo, commit = os.environ["GH_REPO"], os.environ["GITHUB_SHA"]
    version = validator.meta.metadata("aarch64-apple-darwin")["version"]
    tag = "v" + version
    files = validator.validate(directory, validator.meta.TARGETS, commit)
    entries = [{"name":n,"size":(directory/n).stat().st_size,"sha256":validator.sha256(directory/n)} for n in files]
    manifest = directory/"release-manifest.json"
    manifest.write_text(json.dumps(entries, indent=2)+"\n")
    checksum = directory/"SHA256SUMS.txt"
    checksum.write_text("".join(f"{e['sha256']}  {e['name']}\n" for e in entries))
    notes = ROOT/f"docs/releases/{version}.md"
    if not notes.is_file(): raise ValueError("缺少本版本更新说明")
    # A transport/auth/rate-limit failure must never masquerade as a missing tag.
    ref = gh("api", f"repos/{repo}/git/ref/tags/{tag}", optional_404=True)
    if ref is not None:
        obj = json.loads(ref)["object"]
        while obj["type"] == "tag":
            obj = json.loads(gh("api", f"repos/{repo}/git/tags/{obj['sha']}"))["object"]
        if obj["sha"] != commit: raise ValueError("同版本标签已绑定不同提交，请提升 Cargo 版本")
    pages = json.loads(gh("api", f"repos/{repo}/releases?per_page=100", "--paginate", "--slurp"))
    release = next((r for page in pages for r in page if r["tag_name"] == tag), None)
    if release:
        if not release["draft"]: raise ValueError("版本已经公开，禁止修改附件；修复请发布更高版本")
        if release["target_commitish"] != commit: raise ValueError("草稿来自不同提交，请提升版本或明确删除旧草稿")
        names = {a["name"] for a in release["assets"]}
        allowed = set(files) | {manifest.name, checksum.name}
        if names - allowed: raise ValueError("草稿存在未知附件，停止覆盖")
        if names:
            if manifest.name not in names: raise ValueError("已有草稿附件但没有原始发布清单，停止覆盖")
            with tempfile.TemporaryDirectory() as temp:
                gh("release", "download", tag, "--pattern", manifest.name, "--dir", temp)
                if json.loads((pathlib.Path(temp)/manifest.name).read_text()) != entries:
                    raise ValueError("同版本构建内容不同，禁止替换；请提升版本")
    else:
        gh("release", "create", tag, "--target", commit, "--title", f"GuluTwist {version}", "--notes-file", str(notes), "--draft")
    # Upload the binding first so interrupted uploads can only resume identical artifacts.
    gh("release", "upload", tag, str(manifest), "--clobber")
    gh("release", "upload", tag, *[str(directory/n) for n in files], str(checksum), "--clobber")
    gh("release", "edit", tag, "--notes-file", str(notes))
    detail = json.loads(gh("release", "view", tag, "--json", "isDraft,url"))
    if not detail["isDraft"]: raise RuntimeError("发布状态发生变化，请检查 GitHub 上的人工操作")
    message = f"草稿已就绪：{detail['url']}（实机验证后由维护者手动发布）"
    print(message)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as f: f.write(message+"\n")

if __name__ == "__main__":
    main(pathlib.Path(sys.argv[1] if len(sys.argv)>1 else "dist/release").resolve())
