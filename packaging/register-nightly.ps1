<#
.SYNOPSIS
    Registers packaging/nightly.ps1 as a Windows scheduled task, or removes it.

.DESCRIPTION
    The task runs under the current user's account, because the run needs that user's cargo
    toolchain, the MSVC environment, the sealed signing identity and the GitHub credential. A SYSTEM
    task would find none of them.

    It runs the nightly script from the main checkout with the nightly's own worktree as its working
    directory: cargo reads `.cargo/config.toml` from the working directory, and the nightly also sets
    CARGO_TARGET_DIR itself, so no ticket's configuration can move where it builds.

    The older scheduled task `inillucent nightly tier`, which ran only the nightly tier through
    tools/run-nightly.ps1 at 03:00, is removed when this one is registered. The new nightly runs that
    tier too, and appends to the same tests/nightly-history.tsv, so two would run it twice a night.

.PARAMETER At
    The time of day. Default 02:00.

.PARAMETER Worktree
    The nightly worktree, passed through to nightly.ps1. Default C:/build/nightly. It was on J:
    until 2026-10-12. The run of that day took eight hours on J:, which answered at 34 to 37 ms a
    transfer, and eight targets failed their time limits there and passed when run alone on D:.

.PARAMETER Unregister
    Remove the task and exit.

.PARAMETER WhatIf
    Print what would be registered and change nothing.

.EXAMPLE
    pwsh packaging/register-nightly.ps1
    pwsh packaging/register-nightly.ps1 -Unregister
#>
[CmdletBinding()]
param(
    [string] $At = '02:00',
    [string] $Worktree = 'C:/build/nightly',
    [switch] $Unregister,
    [switch] $WhatIf
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'nightly-evidence.ps1')
$main = Get-MainCheckout -Root (Split-Path -Parent $PSScriptRoot)
$taskName = 'inillucent nightly'
$olderTask = 'inillucent nightly tier'
$script = Join-Path $main 'packaging/nightly.ps1'
$arguments = "-NoProfile -File `"$script`" -Worktree `"$Worktree`""
$directory = if (Test-Path -LiteralPath $Worktree) { $Worktree } else { $main }

if ($WhatIf) {
    Write-Host "would register '$taskName' at $At every day: $(Join-Path $env:LOCALAPPDATA 'Microsoft\WindowsApps\pwsh.exe') $arguments"
    Write-Host "   working directory $directory, as $([System.Environment]::UserName)"
    if (Get-ScheduledTask -TaskName $olderTask -ErrorAction SilentlyContinue) {
        Write-Host "would remove the older task '$olderTask'"
    }
    return
}

foreach ($name in @($taskName, $olderTask)) {
    if (Get-ScheduledTask -TaskName $name -ErrorAction SilentlyContinue) {
        Unregister-ScheduledTask -TaskName $name -Confirm:$false
        Write-Host "removed the scheduled task '$name'"
    }
}
if ($Unregister) { return }

# The task names pwsh by its full path. On this machine pwsh is the Microsoft Store package, which
# puts only an alias under %LOCALAPPDATA%\Microsoft\WindowsApps on the user's PATH, and the task
# scheduler does not search that PATH: with -Execute 'pwsh.exe' every start ended with 0x80070002,
# file not found, so no nightly ran on schedule from 2026-09-25 to 2026-10-12. The alias's own path
# works and stays the same when the Store updates PowerShell. The resolved path under
# C:\Program Files\WindowsApps names the package version and would break at the next update.
$alias = Join-Path $env:LOCALAPPDATA 'Microsoft\WindowsApps\pwsh.exe'
$pwsh = if (Test-Path -LiteralPath $alias) { $alias } else { (Get-Command pwsh.exe).Source }
$action = New-ScheduledTaskAction -Execute $pwsh -Argument $arguments -WorkingDirectory $directory
$trigger = New-ScheduledTaskTrigger -Daily -At $At
# Eight hours: the suite, five release builds, notarisation and three gates take most of a night on
# a busy machine, and a run still going at the next night's start is one to stop, not to stack.
$settings = New-ScheduledTaskSettingsSet -StartWhenAvailable -DontStopIfGoingOnBatteries -AllowStartIfOnBatteries `
    -ExecutionTimeLimit (New-TimeSpan -Hours 8) -MultipleInstances IgnoreNew
Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger -Settings $settings `
    -Description 'inillucent nightly: every tier, the release build, the gates, a pre release, latest.json' | Out-Null
Write-Host "registered '$taskName' to run every day at $At"
