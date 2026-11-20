param(
    [string]$MakeNsis = "makensis.exe",
    [string]$VcRuntime
)
$ErrorActionPreference = "Stop"
$project = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Push-Location $project
try {
    if (-not ((& rustc -vV) -contains "host: x86_64-pc-windows-msvc")) {
        throw "Use the x86_64-pc-windows-msvc host toolchain to build this installer."
    }
    & cargo build --release --locked -p musicfs-desktop
    if ($LASTEXITCODE -ne 0) { throw "Rust build failed." }
    $targetDir = (& cargo metadata --locked --offline --no-deps --format-version 1 | ConvertFrom-Json).target_directory
    if (!$targetDir) { throw "Could not locate the Cargo target directory." }
    $version = (Get-Content -LiteralPath (Join-Path $project "VERSION") -Raw).Trim()
    $binary = (Resolve-Path (Join-Path $targetDir "release/musicfs-desktop.exe")).Path
    $output = Join-Path $targetDir "release/MusicFS-${version}-windows-x64-setup.exe"
    $icon = (Resolve-Path "src/app/icons/icon.ico").Path
    if (!$VcRuntime) {
        $redist = $env:VCToolsRedistDir
        if (!$redist) {
            $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio/Installer/vswhere.exe"
            $visualStudio = & $vswhere -latest -products "*" -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
            $redist = Get-ChildItem -Directory (Join-Path $visualStudio "VC/Redist/MSVC") |
                Where-Object Name -match '^\d+\.\d+\.\d+$' |
                Sort-Object { [Version]$_.Name } -Descending | Select-Object -First 1 -ExpandProperty FullName
        }
        if (!$redist) { throw "Could not locate the Visual C++ redistributable directory." }
        $VcRuntime = Get-Item (Join-Path $redist "x64/Microsoft.VC*.CRT/vcruntime140.dll") |
            Select-Object -First 1 -ExpandProperty FullName
    }
    $vcDll = (Resolve-Path -LiteralPath $VcRuntime).Path
    & $MakeNsis /V2 "/DPRODUCT_VERSION=$version" "/DAPP_BINARY=$binary" "/DAPP_ICON=$icon" "/DVC_RUNTIME=$vcDll" "/DSETUP_OUTPUT=$output" "scripts/installer.nsi"
    if ($LASTEXITCODE -ne 0) { throw "Installer build failed." }
    Write-Output "Installer created at $output"
} finally {
    Pop-Location
}
