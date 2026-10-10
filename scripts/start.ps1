$ErrorActionPreference = 'Stop'
$taskProject = Split-Path -Parent $PSScriptRoot
# This local Codex workspace has an isolated compiler. Normal clones use PATH.
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    $taskWorkspace = [System.IO.Path]::GetFullPath((Join-Path $taskProject '..\..'))
    $taskCompiler = Join-Path $taskWorkspace 'work\toolchain'
    if (Test-Path -LiteralPath (Join-Path $taskCompiler 'cargo\bin\cargo.exe')) {
        $env:CARGO_HOME = Join-Path $taskCompiler 'cargo'
        $env:RUSTUP_HOME = Join-Path $taskCompiler 'rustup'
        $env:PATH = (Join-Path $taskCompiler 'cargo\bin') + ';' + (Join-Path $taskCompiler 'mingw64\bin') + ';' + $env:PATH
    } else { throw 'Install Rust and the Tauri platform prerequisites described in README.md first.' }
}
Set-Location -LiteralPath $taskProject
& npm.cmd run desktop
