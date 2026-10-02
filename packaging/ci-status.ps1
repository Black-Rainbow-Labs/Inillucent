<#
.SYNOPSIS
    Waits for GitHub's tests workflow on the public mirror, starting it when the commit has no run,
    and exits 0 only when it passed.

.DESCRIPTION
    A deploy is finished when this exits 0. `ship.ps1` runs it as its last route, and an agent that
    ran any deploy by another route (a mirror push of examples, a site publish, a hand pushed fix)
    runs it before it calls the work done:

        pwsh packaging/ci-status.ps1

    With no arguments it reads the head of `main` on the public mirror (the `brl` remote), starts the
    tests workflow there when that commit has no run, waits for it, prints the verdict with the run's
    URL, and exits 0 when it passed and 1 otherwise. CI runs only on the mirror, at night and by
    hand: the private development repository's workflows are disabled, because its Actions spending
    stopped every job there on 2026-10-02.

    WHY IT EXISTS

    `.github/workflows/tests.yml` stopped parsing on 2026-09-24. Every run on both repositories
    failed in under a second for a week, and eight releases were cut and called finished over those
    red runs, because nothing in the release or in an agent's checklist looked at GitHub.
    `packaging/ci-evidence.ps1` has the rules for reading a run.

.PARAMETER Remote
    Which repositories to read, by git remote. Default brl, the public mirror.

.PARAMETER Sha
    Read this commit on every named repository instead of each one's branch head. The public mirror
    has its own commits, so this is for one repository at a time.

.PARAMETER Branch
    The branch whose head is read. Default main.

.PARAMETER TimeoutMinutes
    How long to wait for a run to finish. Default 180, because a Windows run takes about 70 minutes
    and it can queue. 0 reads once and reports a run still going as not green.

.EXAMPLE
    pwsh packaging/ci-status.ps1
    pwsh packaging/ci-status.ps1 -Remote origin -Sha 1722bc79f1b8a7f5c0d5d3b0b3c8e4a7a2b1c0d9
    pwsh packaging/ci-status.ps1 -TimeoutMinutes 0
#>
[CmdletBinding()]
param(
    [string] $Remote = 'brl',
    [string] $Sha,
    [string] $Branch = 'main',
    [int] $TimeoutMinutes = 180
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'github-token.ps1')
. (Join-Path $PSScriptRoot 'ci-evidence.ps1')

# A comma separated list arrives as one string under `pwsh -File`, so it is split here. The same
# reason ship.ps1 gives for -Only.
$remotes = @($Remote -split ',' | ForEach-Object { $_.Trim() } | Where-Object { $_ })
$red = 0
foreach ($name in $remotes) {
    $repo = Get-RepoFromRemote -Root $root -Remote $name
    $commit = if ($Sha -match "^[0-9a-f]{40}$") { $Sha } elseif ($Sha) { (& git -C $root rev-parse $Sha).Trim() } else { Get-BranchHead -Repo $repo -Branch $Branch }
    Write-Host "$repo at $commit" -ForegroundColor Cyan
    # A run is started on the branch only when the commit read is that branch's head.
    $startOn = if ($Sha) { $null } else { $Branch }
    $verdict = Wait-CiVerdict -Repo $repo -Sha $commit -TimeoutMinutes $TimeoutMinutes -StartOn $startOn
    $colour = if ($verdict.State -eq 'green') { 'Green' } else { 'Red' }
    Write-Host "   $($verdict.State): $($verdict.Reason)" -ForegroundColor $colour
    if ($verdict.State -ne 'green') { $red++ }
}
if ($red -gt 0) {
    Write-Host "$red of $($remotes.Count) repositories are not green. The deploy is not finished." -ForegroundColor Red
    exit 1
}
Write-Host 'the tests workflow is green on every repository read.' -ForegroundColor Green
exit 0
