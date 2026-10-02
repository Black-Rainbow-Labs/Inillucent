<#
.SYNOPSIS
    Reads what GitHub's `tests` workflow said about a commit, on the development repository and on
    the public mirror, and decides whether that commit may be released or called deployed.

.DESCRIPTION
    Dot sourced by `ship.ps1`, whose last route waits for the run of the release on the public
    mirror, and by `ci-status.ps1`, the command an agent runs after any deploy before it calls the
    work finished.

    Since task-2168 the workflows run only on the public mirror, on a schedule and by hand: the
    private development repository's Actions spending stopped every job there on 2026-10-02. So a
    commit a release has just pushed has no run until one is started, and `Wait-CiVerdict -StartOn`
    starts it.

    WHY A RELEASE READS GITHUB AS WELL AS THE NIGHTLY

    From 2026-09-24 `.github/workflows/tests.yml` was not valid YAML, so GitHub refused it and every
    push run on both repositories failed in under a second with no job started. Every release from
    1.0.30 to 2.0.4, eight of them, was cut over those red runs, because nothing in the release
    looked at GitHub: the tests phase read only the nightly's `latest.json`, written on the
    development machine. The nightly does not run clippy, does not run on Linux, and does not read
    the workflow file, so it could not see any of it. The GitHub run is the one check that covers both systems and the file
    itself.

    THE ANSWERS, FROM THE RUNS OF ONE COMMIT

        a run succeeded, and none failed           green
        a run failed, or timed out                 red, naming the run
        every run was cancelled                    red: a newer push superseded it, so nothing graded
                                                   this commit
        a run is queued or in progress             pending: wait
        no run at all                              pending for a few minutes, because GitHub takes a
                                                   moment to start one after a push; then red

    A push of a branch and a push of a tag at the same commit start two runs. Both count, and one
    red run is enough to refuse: they ran the same code.

    ONE EXCEPTION: A RUN THAT STARTED NO JOB, BESIDE ONE THAT PASSED

    A failed run in which no job ran a step graded nothing. That is what a workflow file GitHub
    cannot parse produces, and it is what a private repository produces when its Actions minutes or
    its payment run out: on 2026-10-02 the push of main at 2094ec10 failed in eight seconds with
    "recent account payments have failed or your spending limit needs to be increased", while the
    branch run of the same commit had passed all seven jobs. Such a run is set aside when another
    run of the same commit passed. It cannot hide a broken workflow file, because a file that does
    not parse cannot also produce a passing run of the same commit. With no passing run beside it,
    it stays red.
#>

# The workflow file whose runs are read. A run GitHub could not parse is still reported under this
# path, with no jobs, which is exactly the run this exists to catch.
$script:CiWorkflowPath = '.github/workflows/tests.yml'

function Resolve-CiVerdict {
    <#
    .SYNOPSIS
        Decides what a commit's runs of the tests workflow say. A pure function, so the cases above
        are tested without GitHub (packaging/tests/ci-evidence.Tests.ps1).

    .PARAMETER Runs
        The runs of the tests workflow for one commit, each with status, conclusion, html_url and
        graded_nothing, which says no job of the run started a step.

    .PARAMETER WaitedMinutes
        How long the caller has waited for a run to appear. No run at all is pending for the first
        five minutes after a push and red after that.
    #>
    param([object[]] $Runs, [double] $WaitedMinutes = 0)

    $all = @($Runs | Where-Object { $_ })
    if ($all.Count -eq 0) {
        if ($WaitedMinutes -lt 5) { return @{ State = 'pending'; Reason = 'no tests run has started for this commit yet' } }
        return @{ State = 'red'; Reason = 'no tests run exists for this commit. Was it pushed?' }
    }
    $passedAny = @($all | Where-Object { $_.conclusion -eq 'success' }).Count -gt 0
    $failed = @($all | Where-Object {
            $_.status -eq 'completed' -and
            $_.conclusion -in @('failure', 'timed_out', 'startup_failure', 'action_required') -and
            -not ($passedAny -and $_.graded_nothing)
        })
    if ($failed.Count -gt 0) {
        return @{ State = 'red'; Reason = "the tests run $($failed[0].conclusion): $($failed[0].html_url)" }
    }
    $running = @($all | Where-Object { $_.status -ne 'completed' })
    if ($running.Count -gt 0) {
        return @{ State = 'pending'; Reason = "a tests run is $($running[0].status): $($running[0].html_url)" }
    }
    $passed = @($all | Where-Object { $_.conclusion -eq 'success' })
    if ($passed.Count -gt 0) {
        return @{ State = 'green'; Reason = "the tests run passed: $($passed[0].html_url)" }
    }
    return @{ State = 'red'; Reason = "no tests run for this commit finished: every one was $($all[0].conclusion), so nothing graded it. Push again or re-run it: $($all[0].html_url)" }
}

function Invoke-GitHubApi {
    <#
    .SYNOPSIS
        One GET against the GitHub REST API, with the token git already holds.

    .PARAMETER Path
        The path after https://api.github.com/.

    .PARAMETER Method
        Get, or Post to start a run.

    .PARAMETER Body
        The JSON body of a Post.
    #>
    param([string] $Path, [string] $Method = 'Get', [string] $Body)
    # Resolve-GitHubToken answers nothing when gh has a login of its own, because gh then needs no
    # token. This call is not gh, so it asks gh for the token that login holds.
    $token = Resolve-GitHubToken
    if (-not $token) { $token = (& gh auth token 2>$null) }
    $headers = @{ Accept = 'application/vnd.github+json'; 'X-GitHub-Api-Version' = '2022-11-28' }
    if ($token) { $headers.Authorization = "Bearer $token" }
    if ($Method -eq 'Get') {
        return Invoke-RestMethod -Uri "https://api.github.com/$Path" -Headers $headers -TimeoutSec 60
    }
    return Invoke-RestMethod -Uri "https://api.github.com/$Path" -Headers $headers -TimeoutSec 60 -Method $Method -Body $Body -ContentType 'application/json'
}

function Get-RepoFromRemote {
    <#
    .SYNOPSIS
        The owner/name a git remote points at on GitHub.

    .PARAMETER Root
        A checkout of the repository.

    .PARAMETER Remote
        The remote: `origin` is jasonmcaffee/inillucent, `brl` is the public mirror.
    #>
    param([string] $Root, [string] $Remote)
    $url = (& git -C $Root remote get-url $Remote 2>$null)
    if (-not $url) { throw "this checkout has no `$Remote` remote." }
    if ($url -notmatch 'github\.com[:/](?<owner>[^/]+)/(?<name>[^/]+?)(\.git)?$') { throw "the $Remote remote is $url, which is not a GitHub URL." }
    return "$($Matches.owner)/$($Matches.name)"
}

function Get-BranchHead {
    <#
    .SYNOPSIS
        The commit a branch of a GitHub repository points at now.

    .PARAMETER Repo
        owner/name.

    .PARAMETER Branch
        The branch.
    #>
    param([string] $Repo, [string] $Branch)
    return (Invoke-GitHubApi -Path "repos/$Repo/commits/$Branch").sha
}

function Get-CiRuns {
    <#
    .SYNOPSIS
        Every run of the tests workflow for one commit.

    .PARAMETER Repo
        owner/name.

    .PARAMETER Sha
        The full commit hash.
    #>
    param([string] $Repo, [string] $Sha)
    $answer = Invoke-GitHubApi -Path "repos/$Repo/actions/runs?head_sha=$Sha&per_page=50"
    $runs = @($answer.workflow_runs | Where-Object { $_.path -eq $script:CiWorkflowPath })
    foreach ($run in $runs) {
        # Only a failed run is asked about its jobs, because only a failed one can be set aside.
        $nothing = $false
        if ($run.status -eq 'completed' -and $run.conclusion -ne 'success') {
            $jobs = Invoke-GitHubApi -Path "repos/$Repo/actions/runs/$($run.id)/jobs?per_page=100"
            $nothing = @($jobs.jobs | Where-Object { @($_.steps).Count -gt 0 }).Count -eq 0
        }
        $run | Add-Member -NotePropertyName graded_nothing -NotePropertyValue $nothing -Force
    }
    return $runs
}

function Start-CiRun {
    <#
    .SYNOPSIS
        Starts the tests workflow by hand on a branch of a repository.

    .DESCRIPTION
        The workflow runs only on a schedule and by hand since task-2168, so a commit a release has
        just pushed has no run until one is started. GitHub runs it at the branch's head commit.

    .PARAMETER Repo
        owner/name.

    .PARAMETER Branch
        The branch to run it on.
    #>
    param([string] $Repo, [string] $Branch)
    $workflow = Split-Path -Leaf $script:CiWorkflowPath
    $body = @{ ref = $Branch } | ConvertTo-Json -Compress
    $null = Invoke-GitHubApi -Path "repos/$Repo/actions/workflows/$workflow/dispatches" -Method Post -Body $body
}

function Wait-CiVerdict {
    <#
    .SYNOPSIS
        Waits until a commit's tests runs on one repository have finished, and returns the verdict.

    .DESCRIPTION
        A run takes about 70 minutes on Windows and 50 on Linux, so the default wait is three hours.
        It reads GitHub once a minute and prints a line when the state changes, so a person watching
        can see it is alive.

    .PARAMETER Repo
        owner/name.

    .PARAMETER Sha
        The full commit hash.

    .PARAMETER TimeoutMinutes
        How long to wait for the runs to finish. 0 reads once and returns, pending or not.

    .PARAMETER StartOn
        A branch whose head is `Sha`. When the commit has no run yet, one is started there, once.
    #>
    param([string] $Repo, [string] $Sha, [int] $TimeoutMinutes = 180, [string] $StartOn)
    $started = Get-Date
    $last = ''
    $asked = $false
    while ($true) {
        $waited = ((Get-Date) - $started).TotalMinutes
        $verdict = try {
            $runs = Get-CiRuns -Repo $Repo -Sha $Sha
            if ($runs.Count -eq 0 -and $StartOn -and -not $asked -and $TimeoutMinutes -gt 0) {
                Start-CiRun -Repo $Repo -Branch $StartOn
                $asked = $true
                Write-Host "   $Repo $($Sha.Substring(0, 12)): no tests run yet, so one was started on $StartOn"
            }
            Resolve-CiVerdict -Runs $runs -WaitedMinutes $waited
        } catch {
            @{ State = 'pending'; Reason = "GitHub could not be read: $($_.Exception.Message)" }
        }
        if ($verdict.Reason -ne $last) {
            Write-Host ("   {0} {1}: {2}" -f $Repo, $Sha.Substring(0, 12), $verdict.Reason)
            $last = $verdict.Reason
        }
        if ($verdict.State -ne 'pending') { return $verdict }
        if ($TimeoutMinutes -le 0 -or $waited -ge $TimeoutMinutes) {
            return @{ State = 'red'; Reason = "still not finished after $([int]$waited) minute(s): $($verdict.Reason)" }
        }
        Start-Sleep -Seconds 60
    }
}
