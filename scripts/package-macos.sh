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
version="$(python3 scripts/release-metadata.py "$arch" --field version)"
channel="$(python3 scripts/release-metadata.py "$arch" --field channel)"
rid="$(python3 scripts/release-metadata.py "$arch" --field rid)"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $version" "$bundle/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $version" "$bundle/Contents/Info.plist"
touch "$bundle/Contents" "$bundle"
# vpk signs after injecting UpdateMac and sq.version, before making either archive.
dotnet tool restore
dotnet tool run vpk -- pack --packId app.gulutwist.breakstation --packVersion "$version" \
  --packDir "$bundle" --mainExe gulu-twist --runtime "$rid" --channel "$channel" \
  --packTitle "GuluTwist Break Station" --packAuthors GuluTwist \
  --bundleId app.gulutwist.breakstation --signAppIdentity - --noInst --delta None \
  --releaseNotes "docs/releases/$version.md" --outputDir "$stage/output" --skip-updates -y
mkdir -p "dist/ota/$arch"
# Only checked, fresh output enters the CI artifact directory.
python3 scripts/release-metadata.py "$arch" --output "$stage/output/build.$channel.json"
python3 scripts/validate-ota-artifacts.py "$stage/output" --target "$arch"
mkdir -p "$stage/verify"
archive=("$stage/output/"*Portable.zip)
[[ ${#archive[@]} == 1 ]]
ditto -x -k "${archive[0]}" "$stage/verify"
final_bundle=("$stage/verify/"*.app)
codesign --verify --deep --strict "${final_bundle[0]}"
python3 scripts/verify-macos-icon.py "${final_bundle[0]}"
# The destination only contains generated artifacts, never user data.
rm -rf "dist/ota/$arch"
mv "$stage/output" "dist/ota/$arch"
printf '%s\n' "dist/ota/$arch"
