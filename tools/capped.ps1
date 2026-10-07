<#
.SYNOPSIS
    Runs one command on 80% of the machine's processors, with the MSVC environment imported.

.DESCRIPTION
    `inillucent-testrun`, `ship.ps1`, `release-all.ps1` and `nightly.ps1` confine themselves to a
    share of the machine's logical processors. A cargo command typed by hand does not, and a cold
    `cargo test` or `cargo build --release` takes every processor the machine has. Put this in
    front of it:

        pwsh tools/capped.ps1 cargo test -p inillucent-sql --lib
        pwsh tools/capped.ps1 cargo build --release -p inillucent-cli

    `Enter-ProcessorShare` sets this process's affinity mask, and the command and everything it
    starts inherit it. The MSVC environment is imported first, because `onig_sys` cannot compile
    without it and a raw cargo run from an agent terminal does not have it.

    INILLUCENT_CPU_PERCENT changes the share; 100 runs the command on every processor. The exit
    code is the command's. There is no param block on purpose: one would make this an advanced
    script, and PowerShell would then take an argument such as `-Out` or `-Verbose` meant for the
    command as one of its own common parameters.
#>
$ErrorActionPreference = 'Stop'
if ($args.Count -eq 0) { throw 'usage: pwsh tools/capped.ps1 <program> [arguments...]' }
. (Join-Path (Split-Path -Parent $PSScriptRoot) 'packaging/stage-layout.ps1')
Import-MsvcEnvironment
Enter-ProcessorShare
$program = $args[0]
$rest = @($args | Select-Object -Skip 1)
& $program @rest
exit $LASTEXITCODE
