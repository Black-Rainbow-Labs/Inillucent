<#
.SYNOPSIS
    The checks the release makes around the site's downloads prune.

.DESCRIPTION
    `packaging/site/prune-downloads.ps1` runs the sites repository's prune and then checks
    `SHA256SUMS` against the files left. These cases cover that check, which needs no sites
    repository.

    Written for Pester 3.4, the version Windows ships:

        Invoke-Pester packaging/tests
#>

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
. (Join-Path (Split-Path -Parent $here) 'site/prune-downloads.ps1')

function New-DownloadsFixture {
    <#
    .SYNOPSIS
        A downloads folder with two 2.10.0 files and a SHA256SUMS that names both.
    #>
    $folder = Join-Path ([System.IO.Path]::GetTempPath()) ("prune-downloads-" + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $folder | Out-Null
    $lines = @()
    foreach ($name in @('inillucent-2.10.0.pkg', 'inillucent_2.10.0_amd64.deb')) {
        $path = Join-Path $folder $name
        Set-Content -LiteralPath $path -Value $name
        $lines += "$((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLower())  $name"
    }
    Set-Content -LiteralPath (Join-Path $folder 'SHA256SUMS') -Value $lines
    return $folder
}

Describe 'Test-SiteSums' {
    It 'passes when every file SHA256SUMS names is present with its hash' {
        $folder = New-DownloadsFixture
        Test-SiteSums -Downloads $folder | Should Be $null
        Remove-Item -LiteralPath $folder -Recurse -Force -Confirm:$false
    }

    It 'names a file SHA256SUMS lists that the folder does not have' {
        $folder = New-DownloadsFixture
        Remove-Item -LiteralPath (Join-Path $folder 'inillucent-2.10.0.pkg') -Confirm:$false
        Test-SiteSums -Downloads $folder | Should Be 'SHA256SUMS names inillucent-2.10.0.pkg, and public/downloads does not have it'
        Remove-Item -LiteralPath $folder -Recurse -Force -Confirm:$false
    }

    It 'names a file whose bytes no longer match its line' {
        $folder = New-DownloadsFixture
        Set-Content -LiteralPath (Join-Path $folder 'inillucent_2.10.0_amd64.deb') -Value 'changed'
        Test-SiteSums -Downloads $folder | Should Be 'inillucent_2.10.0_amd64.deb does not match its SHA256SUMS line'
        Remove-Item -LiteralPath $folder -Recurse -Force -Confirm:$false
    }
}
