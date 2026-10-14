#!/usr/bin/env python3
"""One source of release identity; Python 3.11+ on every packaging host."""
import argparse
import json
import pathlib
import subprocess
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
PACKAGE_ID = "app.gulutwist.breakstation"
TARGETS = {
    "aarch64-apple-darwin": ("osx-arm64", "osx-arm64-stable"),
    "x86_64-apple-darwin": ("osx-x64", "osx-x64-stable"),
    "x86_64-pc-windows-msvc": ("win-x64", "win-x64-stable"),
}
def metadata(target):
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    rid, channel = TARGETS[target]
    return dict(version=version, package_id=PACKAGE_ID, target=target, rid=rid, channel=channel,
                commit=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip())
if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("target", choices=TARGETS)
    p.add_argument("--field")
    p.add_argument("--output", type=pathlib.Path)
    a = p.parse_args()
    value = metadata(a.target)
    text = value[a.field] if a.field else json.dumps(value, indent=2)
    if a.output: a.output.write_text(text + "\n")
    else: print(text)
