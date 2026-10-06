<#
.SYNOPSIS
    Builds the Windows release with profile guided optimisation.

.DESCRIPTION
    Three steps, in place of the one `cargo build` the other targets get:

        1. build the release instrumented, into a target directory of its own;
        2. run packaging/pgo/train.mjs against that build, which writes counts
           of what ran into a profile directory;
        3. merge the counts with llvm-profdata and build the release again with
           them, into the target directory the release reads.

    `packaging/release-all.ps1` starts this as the Windows build. It is a
    script of its own so that the parallel build can start it as one process
    with one log, like the cargo builds beside it.

    WHY. Measured on the usage benchmark in docs/performance.md, seven rounds,
    against the same commit built without a profile: Python inserts of 10,000
    rows in a transaction went from 33.99 ms to 28.39 ms, 200 grouped
    aggregates from 118.2 ms to 92.7 ms, the shell's CSV import of 50,000 rows
    from 59.3 ms to 54.7 ms, and a one row insert through the command line from
    24.20 ms to 22.87 ms. No workload was slower by more than its spread. The
    gain is code layout and inlining chosen from real counts instead of
    guesses, so it is largest where a short loop runs many times.

    Only the Windows target. The training has to run the programs it trains,
    and this machine runs only the Windows ones. A profile from one target
    applied to another is accepted by the compiler for the functions whose
    shape matches, and nobody has measured what it does there.

    The training needs node and python on the PATH, as the release machine has.
    Without them this stops: a profile from half the training would mark the
    other half as cold code.

.PARAMETER TargetDir
    Where the final build goes. The instrumented build goes beside it, in
    <TargetDir>-instrumented, and the profiles in <TargetDir>-profile.

.PARAMETER CargoArgs
    The arguments after `cargo build`, without --target-dir, as JSON and then
    base64, because `pwsh -File` passes an array as one string.

.PARAMETER RustFlags
    The `--config target.<triple>.rustflags=[...]` value the plain build would
    use, base64. The profile flag is added to its list.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $TargetDir,
    [Parameter(Mandatory)] [string] $CargoArgs,
    [Parameter(Mandatory)] [string] $RustFlags
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
. (Join-Path $root 'packaging/stage-layout.ps1')

$triple = 'x86_64-pc-windows-msvc'

function ConvertFrom-Base64Text {
    <#
    .SYNOPSIS
        Decodes a base64 parameter back to its text.

    .PARAMETER Text
        The base64 text.
    #>
    param([string] $Text)
    return [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($Text))
}

function Add-RustFlag {
    <#
    .SYNOPSIS
        Returns the rustflags config with one more `-C` flag at the end of its list.

    .PARAMETER Config
        The `target.<triple>.rustflags=[...]` text.

    .PARAMETER Flag
        The codegen option, such as profile-use=<file>.
    #>
    param([string] $Config, [string] $Flag)
    $end = $Config.LastIndexOf(']')
    if ($end -lt 0) { throw "the rustflags config has no list: $Config" }
    return $Config.Substring(0, $end) + ",`"-C`",`"$Flag`"]"
}

function Invoke-ProfileCargo {
    <#
    .SYNOPSIS
        Runs one cargo build of the release and stops on a failure.

    .PARAMETER Arguments
        The arguments after `cargo build`.

    .PARAMETER Config
        The rustflags config for this build.

    .PARAMETER Directory
        The target directory.
    #>
    param([string[]] $Arguments, [string] $Config, [string] $Directory)
    & cargo build @Arguments --config $Config --target-dir $Directory
    if ($LASTEXITCODE -ne 0) { throw "cargo build into $Directory failed with $LASTEXITCODE" }
}

function Get-ProfdataProgram {
    <#
    .SYNOPSIS
        The llvm-profdata that belongs to the pinned compiler.

    .DESCRIPTION
        It has to be the compiler's own copy: the raw profile format changes with LLVM's version,
        and a merge by another LLVM fails or writes a profile the compiler reads as empty.
    #>
    $sysroot = (& rustc --print sysroot).Trim()
    $program = Join-Path $sysroot "lib/rustlib/$triple/bin/llvm-profdata.exe"
    if (-not (Test-Path -LiteralPath $program)) {
        throw "llvm-profdata is missing from $sysroot. Run: rustup component add llvm-tools"
    }
    return $program
}

$arguments = @(ConvertFrom-Base64Text $CargoArgs | ConvertFrom-Json)
$flags = ConvertFrom-Base64Text $RustFlags
$instrumented = "$TargetDir-instrumented"
$profileRoot = "$TargetDir-profile"
$raw = Join-Path $profileRoot 'raw'
$merged = Join-Path $profileRoot 'merged.profdata'
Import-MsvcEnvironment

# Counts from an earlier release would be merged into this one's, so the folder starts empty.
if (Test-Path -LiteralPath $profileRoot) { Remove-Item -LiteralPath $profileRoot -Recurse -Force }
New-Item -ItemType Directory -Force -Path $raw | Out-Null

$clock = [System.Diagnostics.Stopwatch]::StartNew()
Write-Host '== instrumented build'
Invoke-ProfileCargo -Arguments $arguments -Directory $instrumented `
    -Config (Add-RustFlag -Config $flags -Flag ("profile-generate=" + $raw.Replace('\', '/')))
Write-Host ("   {0:N1} s" -f $clock.Elapsed.TotalSeconds)

Write-Host '== training'
& node (Join-Path $root 'packaging/pgo/train.mjs') (Join-Path $instrumented "$triple/release") (Join-Path $profileRoot 'work')
if ($LASTEXITCODE -ne 0) { throw "the training run failed with $LASTEXITCODE" }
$counts = @(Get-ChildItem -LiteralPath $raw -Filter '*.profraw')
if ($counts.Count -eq 0) { throw "the training run wrote no profile into $raw" }
Write-Host ("   {0} profiles, {1:N1} s" -f $counts.Count, $clock.Elapsed.TotalSeconds)

& (Get-ProfdataProgram) merge -o $merged $raw
if ($LASTEXITCODE -ne 0) { throw "llvm-profdata merge failed with $LASTEXITCODE" }

Write-Host '== optimised build'
Invoke-ProfileCargo -Arguments $arguments -Directory $TargetDir `
    -Config (Add-RustFlag -Config $flags -Flag ("profile-use=" + $merged.Replace('\', '/')))
Write-Host ("   done in {0:N1} s" -f $clock.Elapsed.TotalSeconds)
