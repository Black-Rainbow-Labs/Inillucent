<#
.SYNOPSIS
    Which files in dist/ belong to an earlier release that GitHub already holds, so they can go.

.DESCRIPTION
    `release-all.ps1` writes every artifact into dist/ under a versioned name and never removes
    one, so dist/ grows by about 600 MB a release. The release worktree on J: held 2.1.3, 2.1.4
    and 2.1.5 side by side, 1.8 GB, and the main checkout still held 0.1.1 to 1.0.32. The GitHub
    release is the copy that is kept, so a local copy of an earlier version is only a duplicate
    once GitHub has it.

    `Get-DistPrunePlan` decides, and decides nothing it cannot check:

    - an entry whose name carries the version being released is kept. Later routes and an
      `-Only` rerun read it.
    - an entry with no version in its name is kept: `SHA256SUMS`, `SHA256SUMS.minisig`,
      `provenance.json` and `_nfpm` describe the current build.
    - a file of an earlier version is removed only when that version's GitHub release lists a
      file of exactly that name.
    - a file or folder of an earlier version that no release ever carries (the staging folder an
      archive was packed from, the notary's `-apple-darwin.zip`) is removed only when that
      version's GitHub release exists and has at least one asset. It is an intermediate copy of
      what the release already holds.
    - anything else of an earlier version, for example a version GitHub has no release for, is
      kept and reported, because deleting it would delete the only copy.

    The GitHub lookup is passed in as a script block so the cases in
    `packaging/tests/dist-prune.Tests.ps1` run without the network.
#>

function Get-NameVersion {
    <#
    .SYNOPSIS
        The release version a dist/ entry name carries, or $null when it carries none.

    .DESCRIPTION
        Matches the three spellings release-all.ps1 writes: `inillucent-2.1.3-<triple>.tar.gz`,
        `inillucent-2.1.3.pkg` and `inillucent_2.1.3_amd64.deb`.

    .PARAMETER Name
        The file or folder name.
    #>
    param([string] $Name)
    if ($Name -match '^inillucent[-_](\d+\.\d+\.\d+)(?=[-_.]|$)') { return $Matches[1] }
    return $null
}

function Get-DistPrunePlan {
    <#
    .SYNOPSIS
        Splits dist/ into what may be deleted and what is kept, with a reason for each kept entry.

    .PARAMETER Dist
        The dist/ folder.

    .PARAMETER Version
        The version being released. Its entries are always kept.

    .PARAMETER ReleaseAssets
        A script block that takes a version and returns the asset names of its GitHub release,
        an empty array when the release has none, or $null when the release does not exist.
    #>
    param([string] $Dist, [string] $Version, [scriptblock] $ReleaseAssets)
    $remove = @()
    $kept = @()
    if (-not (Test-Path -LiteralPath $Dist)) { return [pscustomobject]@{ Remove = $remove; Kept = $kept } }
    $assetsByVersion = @{}
    foreach ($entry in Get-ChildItem -LiteralPath $Dist -Force) {
        $entryVersion = Get-NameVersion -Name $entry.Name
        if (-not $entryVersion -or $entryVersion -eq $Version) { continue }
        if (-not $assetsByVersion.ContainsKey($entryVersion)) {
            $assetsByVersion[$entryVersion] = & $ReleaseAssets $entryVersion
        }
        $assets = $assetsByVersion[$entryVersion]
        $reason = Get-KeepReason -Entry $entry -EntryVersion $entryVersion -Assets $assets
        if ($reason) {
            $kept += [pscustomobject]@{ Path = $entry.FullName; Reason = $reason }
        } else {
            $remove += $entry
        }
    }
    return [pscustomobject]@{ Remove = $remove; Kept = $kept }
}

function Get-KeepReason {
    <#
    .SYNOPSIS
        Why an earlier version's dist/ entry must stay, or $null when GitHub already has it.

    .PARAMETER Entry
        The file or folder.

    .PARAMETER EntryVersion
        The version its name carries.

    .PARAMETER Assets
        The asset names of that version's GitHub release, or $null when there is no release.
    #>
    param($Entry, [string] $EntryVersion, $Assets)
    if ($null -eq $Assets) { return "GitHub has no v$EntryVersion release, so this may be the only copy" }
    $names = @($Assets)
    if ($names.Count -eq 0) { return "the v$EntryVersion GitHub release has no assets" }
    if ($names -contains $Entry.Name) { return $null }
    # A file with an asset's name pattern that GitHub does not list is a real artifact missing from
    # the release. Only the two kinds of entry no release carries are intermediate copies.
    $intermediate = $Entry.PSIsContainer -or $Entry.Name -like '*-apple-darwin.zip'
    if ($intermediate) { return $null }
    return "the v$EntryVersion GitHub release does not list $($Entry.Name)"
}

function Get-GitHubReleaseAssets {
    <#
    .SYNOPSIS
        The asset names of one GitHub release, or $null when the release does not exist.

    .PARAMETER Version
        The version, without the leading v.

    .PARAMETER Repo
        The owner/name of the repository the releases are on.
    #>
    param([string] $Version, [string] $Repo)
    $names = & gh release view "v$Version" --repo $Repo --json assets --jq '.assets[].name' 2>$null
    if ($LASTEXITCODE -ne 0) { return $null }
    return @($names | Where-Object { $_ })
}

function Invoke-DistPrune {
    <#
    .SYNOPSIS
        Prints the dist/ prune plan and, unless -WhatIf, deletes what it names.

    .PARAMETER Dist
        The dist/ folder.

    .PARAMETER Version
        The version being released.

    .PARAMETER ReleaseAssets
        The GitHub lookup, as Get-DistPrunePlan takes it.

    .PARAMETER WhatIf
        Print the plan and delete nothing.
    #>
    param([string] $Dist, [string] $Version, [scriptblock] $ReleaseAssets, [bool] $WhatIf)
    $plan = Get-DistPrunePlan -Dist $Dist -Version $Version -ReleaseAssets $ReleaseAssets
    $verb = if ($WhatIf) { 'would delete' } else { 'deleting' }
    foreach ($entry in $plan.Remove) {
        Write-Host "   $verb dist/$($entry.Name)"
        if (-not $WhatIf) { Remove-Item -LiteralPath $entry.FullName -Recurse -Force -Confirm:$false }
    }
    foreach ($keep in $plan.Kept) {
        Write-Host "   keeping $($keep.Path): $($keep.Reason)"
    }
    if ($plan.Remove.Count -eq 0) { Write-Host '   dist/ holds no earlier release that GitHub already has' }
    return $plan
}
