<#
.SYNOPSIS
    What a release and a finished deploy do with GitHub's tests runs for one commit.

.DESCRIPTION
    `Resolve-CiVerdict` in `packaging/ci-evidence.ps1` is a pure function of a commit's runs, and
    `ship.ps1` and `ci-status.ps1` both act on its answer. These cases hold each answer, including the
    one that started this: a run GitHub could not parse is reported as a completed failure with no
    jobs, and it has to read as red.

    Written for Pester 3.4, the version Windows ships, so it runs without installing anything:

        Invoke-Pester packaging/tests
#>

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
. (Join-Path (Split-Path -Parent $here) 'ci-evidence.ps1')

function New-Run([string] $Status, [string] $Conclusion, [string] $Url = 'https://example/run', [bool] $Nothing = $false) {
    [pscustomobject]@{ status = $Status; conclusion = $Conclusion; html_url = $Url; graded_nothing = $Nothing }
}

Describe 'Resolve-CiVerdict' {
    It 'is green when a run passed and none failed' {
        $verdict = Resolve-CiVerdict -Runs @((New-Run 'completed' 'success'))
        $verdict.State | Should Be 'green'
    }

    It 'is red for the run GitHub could not parse, which failed with no job' {
        $verdict = Resolve-CiVerdict -Runs @((New-Run 'completed' 'failure' 'https://example/36953854525'))
        $verdict.State | Should Be 'red'
        $verdict.Reason | Should Match '36953854525'
    }

    It 'is red when the branch run passed and the tag run of the same commit failed' {
        $verdict = Resolve-CiVerdict -Runs @((New-Run 'completed' 'success'), (New-Run 'completed' 'failure'))
        $verdict.State | Should Be 'red'
    }

    It 'is green when a newer push cancelled the branch run and the tag run passed' {
        $verdict = Resolve-CiVerdict -Runs @((New-Run 'completed' 'cancelled'), (New-Run 'completed' 'success'))
        $verdict.State | Should Be 'green'
    }

    It 'is red when every run was cancelled, because nothing graded the commit' {
        $verdict = Resolve-CiVerdict -Runs @((New-Run 'completed' 'cancelled'))
        $verdict.State | Should Be 'red'
        $verdict.Reason | Should Match 'nothing graded it'
    }

    It 'waits while a run is in progress' {
        $verdict = Resolve-CiVerdict -Runs @((New-Run 'completed' 'success'), (New-Run 'in_progress' $null))
        $verdict.State | Should Be 'pending'
    }

    It 'waits a few minutes for a run to start after a push, then calls it red' {
        (Resolve-CiVerdict -Runs @() -WaitedMinutes 1).State | Should Be 'pending'
        (Resolve-CiVerdict -Runs @() -WaitedMinutes 6).State | Should Be 'red'
    }

    It 'sets aside a run that started no job when another run of the commit passed' {
        $blocked = New-Run 'completed' 'failure' 'https://example/36998639130' $true
        $verdict = Resolve-CiVerdict -Runs @($blocked, (New-Run 'completed' 'success'))
        $verdict.State | Should Be 'green'
    }

    It 'keeps a run that started no job red when nothing else passed, as a workflow that did not parse' {
        $verdict = Resolve-CiVerdict -Runs @((New-Run 'completed' 'failure' 'https://example/run' $true))
        $verdict.State | Should Be 'red'
    }

    It 'keeps a run whose jobs ran and failed red beside one that passed' {
        $verdict = Resolve-CiVerdict -Runs @((New-Run 'completed' 'failure'), (New-Run 'completed' 'success'))
        $verdict.State | Should Be 'red'
    }

    It 'is red for a run that timed out' {
        (Resolve-CiVerdict -Runs @((New-Run 'completed' 'timed_out'))).State | Should Be 'red'
    }
}
