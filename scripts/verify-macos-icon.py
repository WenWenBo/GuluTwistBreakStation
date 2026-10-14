#!/usr/bin/env python3
"""Check the packaged icon reference and cache invalidation metadata."""
import hashlib
import plistlib
import struct
import sys
from pathlib import Path


def verify(bundle):
    metadata = plistlib.loads((bundle / "Contents/Info.plist").read_bytes())
    assert metadata.get("CFBundleIdentifier") == "app.gulutwist.breakstation", "app identity changed"
    assert metadata.get("CFBundlePackageType") == "APPL", "expected an application bundle"
    assert metadata.get("LSUIElement") is True, "resident app must start as an agent"
    assert not metadata.get("LSBackgroundOnly", False), "resident app must allow windows"
    name = metadata["CFBundleIconFile"]
    icon = bundle / "Contents/Resources" / name
    if not icon.suffix:
        icon = icon.with_suffix(".icns")
    data = icon.read_bytes()
    digest = hashlib.sha256(data).hexdigest()[:12]
    assert name == f"AppIcon-{digest}.icns", "icon filename must change with its content"
    assert data[:4] == b"icns" and struct.unpack(">I", data[4:8])[0] == len(data), "invalid ICNS"
    kinds = set()
    offset = 8
    while offset < len(data):
        kind, size = struct.unpack(">4sI", data[offset:offset + 8])
        assert size >= 8 and offset + size <= len(data), "invalid ICNS entry"
        kinds.add(kind)
        offset += size
    assert kinds & {b"ic04", b"icp4", b"is32"}, "missing 16 px icon"
    assert kinds & {b"ic05", b"icp5", b"il32"}, "missing 32 px icon"
    assert {b"ic07", b"ic08", b"ic09", b"ic10", b"ic11", b"ic12", b"ic13", b"ic14"} <= kinds, "missing icon sizes"
    assert bundle.stat().st_mtime >= icon.stat().st_mtime, "stale bundle modification time"
    assert list((bundle / "Contents/Resources").glob("AppIcon*.icns")) == [icon], "stale icon resource"
    print(f"icon verified: {name}, build {metadata['CFBundleVersion']}")


if __name__ == "__main__":
    verify(Path(sys.argv[1]))
