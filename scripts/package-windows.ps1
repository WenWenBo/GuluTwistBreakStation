$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
$target = 'x86_64-pc-windows-msvc'
$meta = python scripts/release-metadata.py $target | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Release metadata failed' }
rustup target add $target
if ($LASTEXITCODE -ne 0) { throw 'Rust target installation failed' }
cargo build --locked --release --target $target -p gulu-app -j 4
if ($LASTEXITCODE -ne 0) { throw 'Windows build failed' }
$stage = Join-Path ([System.IO.Path]::GetTempPath()) ('gulu-package-' + [guid]::NewGuid())
try {
    New-Item -ItemType Directory -Force "$stage/app" | Out-Null
    Copy-Item "target/$target/release/gulu-twist.exe" "$stage/app/"
    dotnet tool restore
    if ($LASTEXITCODE -ne 0) { throw 'vpk restore failed' }
    dotnet tool run vpk -- pack --packId $meta.package_id --packVersion $meta.version `
      --packDir "$stage/app" --mainExe gulu-twist.exe --runtime $meta.rid --channel $meta.channel `
      --packTitle 'GuluTwist Break Station' --packAuthors GuluTwist --icon assets/icons/app.ico `
      --aumid app.gulutwist.breakstation --instLocation PerUser --noPortable --delta None --releaseNotes "docs/releases/$($meta.version).md" `
      --outputDir "$stage/output" --skip-updates -y
    if ($LASTEXITCODE -ne 0) { throw 'Velopack packaging failed' }
    python scripts/release-metadata.py $target --output "$stage/output/build.$($meta.channel).json"
    if ($LASTEXITCODE -ne 0) { throw 'Build metadata failed' }
    python scripts/validate-ota-artifacts.py "$stage/output" --target $target
    if ($LASTEXITCODE -ne 0) { throw 'Package validation failed' }
    New-Item -ItemType Directory -Force dist/ota | Out-Null
    if (Test-Path "dist/ota/$target") { Remove-Item -Recurse -Force "dist/ota/$target" }
    Move-Item "$stage/output" "dist/ota/$target"
} finally { Remove-Item -Recurse -Force $stage }
