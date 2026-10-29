<#
.SYNOPSIS
  Build the Windows release into dist\release:
    tanki-skin-studio-<version>-windows-<arch>.msi            installer for the current user
    tanki-skin-studio-<version>-windows-<arch>-portable.zip   unzip anywhere and run

.DESCRIPTION
  Both hold tanki-studio.exe (the app), tanki-cli.exe and the licences. The binaries link the
  C runtime statically (+crt-static), so no Visual C++ redistributable is needed. Needs Rust with
  the MSVC toolchain and the target for -Arch, and WiX v5:
    dotnet tool install --global wix --version 5.0.2

  Signing comes with a code-signing certificate (see docs\releasing.md); until then the files are
  unsigned and Windows SmartScreen asks once before the first start.

.EXAMPLE
  ./packaging/windows/package.ps1 -Arch x64 -Version 0.1.0
#>
param(
  [ValidateSet('x64', 'arm64')]
  [string] $Arch = 'x64',
  [string] $Version = ''
)
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..\..')

$Target = @{ x64 = 'x86_64-pc-windows-msvc'; arm64 = 'aarch64-pc-windows-msvc' }[$Arch]
if (-not $Version) {
  $Version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"$' | Select-Object -First 1).Matches[0].Groups[1].Value
}
if ($Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$') {
  throw "not a version: $Version"
}

# With --target, RUSTFLAGS reach only the target binaries, not build scripts.
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo build --release --locked --target $Target -p tanki-studio -p tanki-cli
if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }

$Bin = "target\$Target\release"
$Name = "tanki-skin-studio-$Version-windows-$Arch"
$Stage = "target\windows-package\$Name"
$Dist = 'dist\release'
Remove-Item -Recurse -Force $Stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $Stage, "$Stage\licenses", $Dist | Out-Null

Copy-Item "$Bin\tanki-studio.exe", "$Bin\tanki-cli.exe" $Stage
Copy-Item LICENSE "$Stage\LICENSE.txt"
Copy-Item crates\tanki-ui\assets\OFL-*.txt, crates\tanki-ui\assets\LICENSE-lucide.txt "$Stage\licenses"

if ($env:GITHUB_ACTIONS) {
  Write-Output '::warning::Windows: no code-signing certificate yet, so the executables are unsigned'
} else {
  Write-Warning 'Windows: no code-signing certificate yet, so the executables are unsigned'
}

# The installer: per user, no administrator rights (tanki-skin-studio.wxs). An MSI version is
# numbers only, so a pre-release such as 0.2.0-rc.1 installs as 0.2.0.
$MsiVersion = $Version -replace '-.*$', ''
$Msi = Join-Path $Dist "$Name.msi"
wix build (Join-Path $PSScriptRoot 'tanki-skin-studio.wxs') -arch $Arch `
  -d "Version=$MsiVersion" -d "Stage=$((Resolve-Path $Stage).Path)" -o $Msi
if ($LASTEXITCODE -ne 0) { throw "wix build failed with exit code $LASTEXITCODE" }
# wix writes debug symbols beside the MSI; they are not a download.
Remove-Item -Force -ErrorAction SilentlyContinue ([IO.Path]::ChangeExtension($Msi, '.wixpdb'))
Write-Output "Created $Msi"

# The portable build: unzip anywhere and run tanki-studio.exe.
$Zip = Join-Path $Dist "$Name-portable.zip"
Remove-Item -Force $Zip -ErrorAction SilentlyContinue
Compress-Archive -Path "$Stage\*" -DestinationPath $Zip
Write-Output "Created $Zip"
