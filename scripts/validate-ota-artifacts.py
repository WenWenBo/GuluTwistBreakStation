#!/usr/bin/env python3
"""Fail closed before upload: version, target, complete payload, hashes, archive layout."""
import argparse
import hashlib
import importlib.util
import json
import pathlib
import plistlib
import re
import struct
import stat
import zipfile
import xml.etree.ElementTree as ET

spec = importlib.util.spec_from_file_location("release_metadata", pathlib.Path(__file__).with_name("release-metadata.py"))
meta = importlib.util.module_from_spec(spec)
spec.loader.exec_module(meta)

def sha256(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()

def require(condition, message):
    if not condition: raise ValueError(message)

def safe_name(name):
    return bool(name) and not name.startswith(".") and not any(c in name for c in "/\\:")

def validate_archive(path, target, version):
    with zipfile.ZipFile(path) as z:
        require(z.testzip() is None, f"损坏的归档：{path.name}")
        names = z.namelist()
        require(all(not n.startswith("/") and ".." not in pathlib.PurePosixPath(n).parts and "\\" not in n for n in names), "归档路径无效")
        exe = "gulu-twist.exe" if target.startswith("x86_64-pc") else "gulu-twist"
        binaries = [n for n in names if n.endswith("/" + exe)]
        require(len(binaries) == 1, f"{path.name} 缺少唯一的 {exe}")
        data = z.read(binaries[0])
        if exe.endswith(".exe"):
            require(data[:2] == b"MZ", "非 Windows PE")
            offset = struct.unpack_from("<I", data, 0x3c)[0]
            require(data[offset:offset+4] == b"PE\0\0" and struct.unpack_from("<H",data,offset+4)[0] == 0x8664, "非 Windows x64")
            require(any(n.endswith("/Squirrel.exe") or n.endswith("/Update.exe") for n in names), "Windows 包缺少更新器")
        else:
            require(data[:4] == b"\xcf\xfa\xed\xfe", "非 64 位 Mach-O")
            expected = 0x100000c if target.startswith("aarch64") else 0x1000007
            require(struct.unpack_from("<I",data,4)[0] == expected, "Mac 应用架构不匹配")
            require(any(n.endswith("/UpdateMac") for n in names), "Mac 包缺少 UpdateMac")
            plists = [n for n in names if n.endswith("/Contents/Info.plist")]
            require(len(plists) == 1, "缺少唯一的 Info.plist")
            p = plistlib.loads(z.read(plists[0]))
            require(p["CFBundleIdentifier"] == meta.PACKAGE_ID and p["CFBundleShortVersionString"] == version and p["LSUIElement"], "Mac 身份、版本或常驻模式错误")
        # Windows creates current/sq.version during installation from the root nuspec.
        # macOS archives already contain sq.version (and the portable ZIP has a symlink).
        manifests = [n for n in names if n.endswith("/sq.version") and not stat.S_ISLNK(z.getinfo(n).external_attr >> 16)]
        if path.suffix == ".nupkg":
            nuspec = [n for n in names if n.endswith(".nuspec") and "/" not in n]
            require(len(nuspec) == 1, "缺少唯一的包清单")
            manifests.extend(nuspec)
        require(manifests, "缺少 Velopack 清单")
        for manifest in manifests:
            tree = ET.fromstring(z.read(manifest))
            values = {node.tag.split("}")[-1]: node.text for node in tree.iter()}
            require(values.get("id") == meta.PACKAGE_ID and values.get("version") == version, "安装清单身份或版本错误")
            require(values.get("channel") == meta.TARGETS[target][1], "安装清单 channel 不匹配")
            if target.startswith("x86_64-pc"):
                require(values.get("shortcutAumid") == meta.PACKAGE_ID, "Windows 快捷方式 AUMID 不匹配")


def validate(directory, targets, commit=None):
    files = set()
    for target in targets:
        expected = meta.metadata(target)
        channel = expected["channel"]
        version = expected["version"]
        build_path = directory / f"build.{channel}.json"
        build = json.loads(build_path.read_text())
        for key in ("version", "target", "rid", "channel", "package_id"):
            require(build.get(key) == expected[key], f"{build_path.name}: {key} 不匹配")
        require(re.fullmatch(r"[a-f0-9]{40}", build["commit"]) is not None, "无效源码 SHA")
        if commit: require(build["commit"] == commit, "三个目标必须来自本次源码 SHA")
        feed_path = directory / f"releases.{channel}.json"
        feed = json.loads(feed_path.read_text())
        assets = feed.get("Assets", [])
        require(len(assets) == 1, f"{channel}: 必须恰好有一个全量包")
        asset = assets[0]
        name = asset["FileName"]
        require(safe_name(name) and name.endswith("-full.nupkg") and channel in name, "无效或跨渠道重名的包名")
        require(asset["Type"] == "Full" and asset["Version"] == version and asset["PackageId"] == meta.PACKAGE_ID, "包版本、类型或身份错误")
        package = directory / name
        require(package.stat().st_size == asset["Size"], "包大小不匹配")
        require(sha256(package).lower() == asset["SHA256"].lower(), "包 SHA256 不匹配")
        validate_archive(package, target, version)
        suffix = "Setup.exe" if target.startswith("x86_64-pc") else "Portable.zip"
        initial = list(directory.glob(f"*{channel}*{suffix}"))
        require(len(initial) == 1 and initial[0].stat().st_size > 0, f"{channel}: 缺少唯一的首次安装包")
        if suffix.endswith("zip"): validate_archive(initial[0], target, version)
        else: require(initial[0].read_bytes()[:2] == b"MZ", "安装器不是 PE 文件")
        selected = {build_path.name, feed_path.name, package.name, initial[0].name}
        require(not files.intersection(selected), "跨目标附件重名")
        files.update(selected)
    return sorted(files)

if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("directory", type=pathlib.Path)
    p.add_argument("--target", choices=meta.TARGETS)
    p.add_argument("--commit")
    p.add_argument("--manifest", action="store_true")
    a = p.parse_args()
    names = validate(a.directory, [a.target] if a.target else meta.TARGETS, a.commit)
    if a.manifest:
        entries = [{"name":n,"size":(a.directory/n).stat().st_size,"sha256":sha256(a.directory/n)} for n in names]
        (a.directory/"release-manifest.json").write_text(json.dumps(entries, indent=2)+"\n")
        (a.directory/"SHA256SUMS.txt").write_text("".join(f"{e['sha256']}  {e['name']}\n" for e in entries))
    print("已验证：" + ", ".join(names))
