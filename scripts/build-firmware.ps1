# Build the firmware from any caller directory, including with --manifest-path.
# Cargo discovers config from the current directory and its ancestors, not from
# the manifest, so pass the existing workspace config explicitly.
[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]] $CargoArgs
)

$ErrorActionPreference = "Stop"
$repoDir = (Resolve-Path (Join-Path $PSScriptRoot ".."))
$config = Join-Path $repoDir ".cargo/config.toml"
$manifest = Join-Path $repoDir "Cargo.toml"

$insideRepo = $false
$currentDir = (Get-Location).Path
if ($currentDir -eq $repoDir.Path -or $currentDir.StartsWith($repoDir.Path + [IO.Path]::DirectorySeparatorChar)) {
    $insideRepo = $true
}

if ($insideRepo) {
    & cargo build `
        --manifest-path $manifest `
        --package sensor-watch `
        --bin sensor-watch `
        --release `
        --target thumbv6m-none-eabi `
        @CargoArgs
} else {
    & cargo --config $config build `
        --manifest-path $manifest `
        --package sensor-watch `
        --bin sensor-watch `
        --release `
        --target thumbv6m-none-eabi `
        @CargoArgs
}
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
