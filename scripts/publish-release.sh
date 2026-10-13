#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."

: "${GH_TOKEN:?GitHub token is required}"
: "${GH_REPO:?GitHub repository is required}"
: "${GITHUB_SHA:?Build commit is required}"
: "${GITHUB_RUN_NUMBER:?Workflow run number is required}"
: "${GITHUB_RUN_ID:?Workflow run ID is required}"
: "${GITHUB_STEP_SUMMARY:?Workflow summary path is required}"

artifact_dir="${1:-dist/release}"
version="$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["workspace"]["package"]["version"])')"
tag="v${version}-build.${GITHUB_RUN_NUMBER}"
shopt -s nullglob
arm_packages=("$artifact_dir"/GuluTwist-"$version"-build*-aarch64-apple-darwin.zip)
intel_packages=("$artifact_dir"/GuluTwist-"$version"-build*-x86_64-apple-darwin.zip)
windows_packages=("$artifact_dir"/GuluTwist-"$version"-Windows-x64-portable.zip)
if (( ${#arm_packages[@]} != 1 || ${#intel_packages[@]} != 1 || ${#windows_packages[@]} != 1 )); then
  echo 'Expected one package for each of macOS arm64, macOS Intel and Windows x64.' >&2
  exit 1
fi
packages=("${arm_packages[0]}" "${intel_packages[0]}" "${windows_packages[0]}")
for package in "${packages[@]}"; do
  test -s "$package"
  unzip -tq "$package"
done
(
  cd "$artifact_dir"
  sha256sum -- "$(basename "${packages[0]}")" "$(basename "${packages[1]}")" "$(basename "${packages[2]}")" > SHA256SUMS.txt
)

notes="$artifact_dir/release-notes.md"
cat > "$notes" <<EOF
## 下载

- Apple Silicon Mac：文件名含 aarch64-apple-darwin 的 ZIP，macOS 15+。
- Intel Mac：文件名含 x86_64-apple-darwin 的 ZIP，macOS 15+。
- Windows x64：Windows-x64-portable.zip，解压后运行 gulu-twist.exe。
- SHA256SUMS.txt：以上三份安装包的 SHA-256 校验值。

应用版本：${version}。发布序号：${GITHUB_RUN_NUMBER}（与 Mac 应用内的 build 号独立）。
源码提交：${GITHUB_SHA}。
[构建记录](${GITHUB_SERVER_URL:-https://github.com}/$GH_REPO/actions/runs/$GITHUB_RUN_ID)

Mac 包使用 ad-hoc 签名，尚未进行开发者签名和公证；Windows 包为便携版。平台功能验证范围见源码中的 docs/validation/ 文档。
EOF

# Keep a failed upload as a draft; retries resume the same release.
if draft="$(gh release view "$tag" --json isDraft --jq '.isDraft')"; then
  if [[ "$draft" == false ]]; then
    echo "Release $tag is already published; keeping its existing assets."
  else
    gh release upload "$tag" "${packages[@]}" "$artifact_dir/SHA256SUMS.txt" --clobber
    gh release edit "$tag" --draft=false --notes-file "$notes"
  fi
else
  gh release create "$tag" --target "$GITHUB_SHA" \
    --title "GuluTwist $version · Build $GITHUB_RUN_NUMBER" --notes-file "$notes" --draft
  gh release upload "$tag" "${packages[@]}" "$artifact_dir/SHA256SUMS.txt" --clobber
  gh release edit "$tag" --draft=false
fi
release_url="$(gh release view "$tag" --json url --jq '.url')"
printf 'Release: [%s](%s)\n' "$tag" "$release_url" >> "$GITHUB_STEP_SUMMARY"
