param([switch]$SkipInstaller)
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
rustup target add x86_64-pc-windows-msvc
if ($LASTEXITCODE -ne 0) { throw 'Rust target installation failed' }
cargo build --locked --release --target x86_64-pc-windows-msvc -p gulu-app -j 4
if ($LASTEXITCODE -ne 0) { throw 'Windows build failed' }
New-Item -ItemType Directory -Force dist | Out-Null
Compress-Archive -Force target/x86_64-pc-windows-msvc/release/gulu-twist.exe,assets/icons/app.ico dist/GuluTwist-0.1.0-Windows-x64-portable.zip
if (!$SkipInstaller) {
    $iscc = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if (!$iscc) {
        $candidate = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6/ISCC.exe'
        if (!(Test-Path $candidate)) { throw 'Install Inno Setup 6, or pass -SkipInstaller for a portable build.' }
        $iscc = $candidate
    }
    & $iscc packaging/windows/installer.iss
    if ($LASTEXITCODE -ne 0) { throw 'Installer build failed' }
}
