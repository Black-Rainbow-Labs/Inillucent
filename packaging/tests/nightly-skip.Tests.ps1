<#
.SYNOPSIS
    When the nightly builds nothing because no code changed.

.DESCRIPTION
    `nightly.ps1` exits before building when `Test-NightlyCodeUnchanged` says the code is the same as
    the commit latest.json records. These cases build a real repository in the temporary folder and
    hold each answer: a commit of only the nightly's own records is no change, a source file is a
    change, and a commit the repository does not have, or no commit at all, is a change.

    Written for Pester 3.4, the version Windows ships, so it runs without installing anything:

        Invoke-Pester packaging/tests
#>

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
. (Join-Path (Split-Path -Parent $here) 'nightly-evidence.ps1')

function New-FixtureRepository {
    <#
    .SYNOPSIS
        Makes a repository with one source commit and returns its path and that commit.
    #>
    $repo = Join-Path ([System.IO.Path]::GetTempPath()) ("nightly-skip-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
    New-Item -ItemType Directory -Path (Join-Path $repo 'src') -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $repo 'tests') -Force | Out-Null
    & git -C $repo init -q
    Set-Content -LiteralPath (Join-Path $repo 'src/lib.rs') -Value 'fn one() {}'
    Set-Content -LiteralPath (Join-Path $repo 'tests/timings.toml') -Value 'a = 1'
    Save-FixtureCommit $repo 'code'
    return @{ Path = $repo; First = "$(& git -C $repo rev-parse HEAD)".Trim() }
}

function Save-FixtureCommit {
    <#
    .SYNOPSIS
        Commits everything in the fixture repository and returns the new commit.

    .PARAMETER Repo
        The fixture repository.

    .PARAMETER Message
        The commit message.
    #>
    param([string] $Repo, [string] $Message)
    & git -C $Repo add -A
    & git -C $Repo -c user.name=t -c user.email=t@t commit -q -m $Message
    return "$(& git -C $Repo rev-parse HEAD)".Trim()
}

Describe 'Test-NightlyCodeUnchanged' {
    It 'says unchanged for the same commit' {
        $f = New-FixtureRepository
        Test-NightlyCodeUnchanged -Repository $f.Path -Since $f.First -Commit $f.First | Should Be $true
        Remove-Item -LiteralPath $f.Path -Recurse -Force
    }

    It 'says unchanged when the only new commit is the nightly recording its timings' {
        $f = New-FixtureRepository
        Set-Content -LiteralPath (Join-Path $f.Path 'tests/timings.toml') -Value 'a = 2'
        Set-Content -LiteralPath (Join-Path $f.Path 'tests/nightly-history.tsv') -Value 'row'
        $records = Save-FixtureCommit $f.Path 'nightly: timings'
        Test-NightlyCodeUnchanged -Repository $f.Path -Since $f.First -Commit $records | Should Be $true
        Remove-Item -LiteralPath $f.Path -Recurse -Force
    }

    It 'says changed when a source file changed' {
        $f = New-FixtureRepository
        Set-Content -LiteralPath (Join-Path $f.Path 'src/lib.rs') -Value 'fn two() {}'
        $code = Save-FixtureCommit $f.Path 'code change'
        Test-NightlyCodeUnchanged -Repository $f.Path -Since $f.First -Commit $code | Should Be $false
        Remove-Item -LiteralPath $f.Path -Recurse -Force
    }

    It 'says changed when the last night ran a commit this repository does not have' {
        $f = New-FixtureRepository
        Test-NightlyCodeUnchanged -Repository $f.Path -Since ('c' * 40) -Commit $f.First | Should Be $false
        Remove-Item -LiteralPath $f.Path -Recurse -Force
    }

    It 'says changed when there was no last night' {
        $f = New-FixtureRepository
        Test-NightlyCodeUnchanged -Repository $f.Path -Since '' -Commit $f.First | Should Be $false
        Remove-Item -LiteralPath $f.Path -Recurse -Force
    }
}
