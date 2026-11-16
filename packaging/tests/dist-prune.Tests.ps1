<#
.SYNOPSIS
    Which dist/ entries a release deletes once GitHub holds them.

.DESCRIPTION
    `Get-DistPrunePlan` in `packaging/dist-prune.ps1` deletes a local copy of an earlier release
    only when GitHub already has that release. These cases build a dist/ folder in a temporary
    directory and answer the GitHub lookup from a table, so they need no network.

    Written for Pester 3.4, the version Windows ships:

        Invoke-Pester packaging/tests
#>

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
. (Join-Path (Split-Path -Parent $here) 'dist-prune.ps1')

function New-DistFixture {
    <#
    .SYNOPSIS
        A dist/ folder holding two earlier versions, the current one and the unversioned files.
    #>
    $dist = Join-Path ([System.IO.Path]::GetTempPath()) ("dist-prune-" + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $dist | Out-Null
    $files = @(
        'SHA256SUMS', 'SHA256SUMS.minisig', 'provenance.json',
        'inillucent-2.1.4-x86_64-pc-windows-msvc.zip', 'inillucent-2.1.4.pkg', 'inillucent_2.1.4_amd64.deb',
        'inillucent-2.1.4-universal-apple-darwin.zip',
        'inillucent-2.1.3-x86_64-pc-windows-msvc.zip',
        'inillucent-2.1.5-x86_64-pc-windows-msvc.zip', 'inillucent-2.1.5.pkg'
    )
    foreach ($file in $files) { Set-Content -LiteralPath (Join-Path $dist $file) -Value 'x' }
    foreach ($folder in @('_nfpm', 'inillucent-2.1.4-x86_64-pc-windows-msvc', 'inillucent-2.1.5-x86_64-pc-windows-msvc')) {
        New-Item -ItemType Directory -Path (Join-Path $dist $folder) | Out-Null
    }
    return $dist
}

$releases = @{
    '2.1.4' = @('inillucent-2.1.4-x86_64-pc-windows-msvc.zip', 'inillucent-2.1.4.pkg', 'inillucent_2.1.4_amd64.deb', 'SHA256SUMS')
}
$lookup = { param($v) if ($releases.ContainsKey($v)) { return $releases[$v] } return $null }

Describe 'Get-NameVersion' {
    It 'reads the version from each spelling release-all.ps1 writes' {
        Get-NameVersion -Name 'inillucent-2.10.0-x86_64-pc-windows-msvc.zip' | Should Be '2.10.0'
        Get-NameVersion -Name 'inillucent-2.10.0.pkg' | Should Be '2.10.0'
        Get-NameVersion -Name 'inillucent_2.10.0_arm64.deb' | Should Be '2.10.0'
        Get-NameVersion -Name 'inillucent-2.10.0.aarch64.rpm' | Should Be '2.10.0'
    }

    It 'reads no version from an unversioned name' {
        Get-NameVersion -Name 'SHA256SUMS' | Should Be $null
        Get-NameVersion -Name '_nfpm' | Should Be $null
    }
}

Describe 'Get-DistPrunePlan' {
    $dist = New-DistFixture
    $plan = Get-DistPrunePlan -Dist $dist -Version '2.1.5' -ReleaseAssets $lookup
    $removed = @($plan.Remove | ForEach-Object { $_.Name } | Sort-Object)
    $kept = @($plan.Kept | ForEach-Object { Split-Path -Leaf $_.Path } | Sort-Object)

    It 'deletes the earlier release files GitHub lists, and the staging folder and notary zip beside them' {
        $removed | Should Be @(
            'inillucent_2.1.4_amd64.deb',
            'inillucent-2.1.4.pkg',
            'inillucent-2.1.4-universal-apple-darwin.zip',
            'inillucent-2.1.4-x86_64-pc-windows-msvc',
            'inillucent-2.1.4-x86_64-pc-windows-msvc.zip'
        ) | Sort-Object
    }

    It 'keeps a version GitHub has no release for, and says why' {
        $kept | Should Be @('inillucent-2.1.3-x86_64-pc-windows-msvc.zip')
        $plan.Kept[0].Reason | Should Match 'no v2\.1\.3 release'
    }

    It 'never names the current version or an unversioned entry' {
        $all = @($removed) + @($kept)
        @($all | Where-Object { $_ -like '*2.1.5*' -or $_ -notlike 'inillucent*' }).Count | Should Be 0
    }

    It 'keeps a file of an earlier version that the release does not list' {
        Set-Content -LiteralPath (Join-Path $dist 'inillucent-2.1.4.x86_64.rpm') -Value 'x'
        $again = Get-DistPrunePlan -Dist $dist -Version '2.1.5' -ReleaseAssets $lookup
        $rpm = @($again.Kept | Where-Object { $_.Path -like '*2.1.4.x86_64.rpm' })
        $rpm.Count | Should Be 1
        $rpm[0].Reason | Should Match 'does not list'
    }

    It 'deletes what the plan names, and only that, when run for real' {
        $null = Invoke-DistPrune -Dist $dist -Version '2.1.5' -ReleaseAssets $lookup -WhatIf $false
        $left = @(Get-ChildItem -LiteralPath $dist -Force | ForEach-Object { $_.Name } | Sort-Object)
        $left | Should Be (@(
            '_nfpm', 'inillucent-2.1.3-x86_64-pc-windows-msvc.zip', 'inillucent-2.1.4.x86_64.rpm',
            'inillucent-2.1.5-x86_64-pc-windows-msvc', 'inillucent-2.1.5-x86_64-pc-windows-msvc.zip',
            'inillucent-2.1.5.pkg', 'provenance.json', 'SHA256SUMS', 'SHA256SUMS.minisig'
        ) | Sort-Object)
    }

    It 'deletes nothing under -WhatIf' {
        $second = New-DistFixture
        $before = @(Get-ChildItem -LiteralPath $second -Force).Count
        $null = Invoke-DistPrune -Dist $second -Version '2.1.5' -ReleaseAssets $lookup -WhatIf $true
        @(Get-ChildItem -LiteralPath $second -Force).Count | Should Be $before
        Remove-Item -LiteralPath $second -Recurse -Force -Confirm:$false
    }

    Remove-Item -LiteralPath $dist -Recurse -Force -Confirm:$false
}
