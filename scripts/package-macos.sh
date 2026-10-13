#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
arch="${1:-aarch64-apple-darwin}"
profile="${2:-release}"
case "$arch" in aarch64-apple-darwin|x86_64-apple-darwin) ;; *) exit 2;; esac
export MACOSX_DEPLOYMENT_TARGET=15.0
if [[ "$profile" == debug ]]; then
  cargo build --locked --target "$arch" -p gulu-app -j 4
else
  cargo build --locked --release --target "$arch" -p gulu-app -j 4
fi
mkdir -p "dist/$arch"
stage="$(mktemp -d "dist/$arch/.package.XXXXXX")"
trap 'rm -rf "$stage"' EXIT
bundle="$stage/GuluTwist Break Station.app"
iconset="$stage/AppIcon.iconset"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources" "$iconset"
cp "target/$arch/$profile/gulu-twist" "$bundle/Contents/MacOS/"
cp packaging/macos/Info.plist "$bundle/Contents/"
for size in 16 32 128 256 512; do
  cp "assets/icons/sizes/app-${size}.png" "$iconset/icon_${size}x${size}.png"
  double=$((size * 2))
  cp "assets/icons/sizes/app-${double}.png" "$iconset/icon_${size}x${size}@2x.png"
done
iconutil -c icns "$iconset" -o "$stage/AppIcon.icns"
# Give changed artwork a new resource identity as well as fresh bundle metadata.
icon_hash="$(shasum -a 256 "$stage/AppIcon.icns" | cut -c 1-12)"
icon_name="AppIcon-$icon_hash.icns"
mv "$stage/AppIcon.icns" "$bundle/Contents/Resources/$icon_name"
/usr/libexec/PlistBuddy -c "Set :CFBundleIconFile $icon_name" "$bundle/Contents/Info.plist"
codesign --force --deep --sign - "$bundle"
codesign --verify --deep --strict "$bundle"
touch "$bundle/Contents" "$bundle"
python3 scripts/verify-macos-icon.py "$bundle"
version="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$bundle/Contents/Info.plist")"
build="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$bundle/Contents/Info.plist")"
archive="dist/GuluTwist-$version-build$build-$arch.zip"
ditto -c -k --sequesterRsrc --keepParent "$bundle" "$stage/package.zip"
# Publish only a fully built and checked bundle, without stale files from old builds.
destination="dist/$arch/GuluTwist Break Station.app"
if [[ -e "$destination" ]]; then
  mv "$destination" "$stage/previous.app"
fi
mv "$bundle" "$destination"
mv "$stage/package.zip" "$archive"
# Keep previously shared download paths pointing to the latest build as well.
cp "$archive" "dist/GuluTwist-$version-$arch.zip"
bundle="$destination"
printf '%s\n' "$bundle"
printf '%s\n' "$archive"
