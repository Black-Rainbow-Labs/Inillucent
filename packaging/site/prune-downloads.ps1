<#
.SYNOPSIS
    Keeps inillucent.com's downloads folder to the current release and the three before it.

.DESCRIPTION
    Every release copies about 300 MB of installers into the site's `public/downloads`, and nothing
    removed an old one: the folder held 11.9 GB, and the static export doubled it in `out/downloads`.
    The prune itself is the shared script in the sites repository,
    `sites/scripts/prune-downloads.mjs`, which the unluminous release calls too. It keeps the
    current version, the three versions before it, and the version `src/data/content.ts` links,
    and it leaves every file without a version in its name alone.

    This file is the release's side of it, dot sourced by `publish-site.ps1` (which runs the prune
    in the step that copies the installers in) and by `ship.ps1` (whose -WhatIf prints what the
    prune would delete). After a real prune it checks that `SHA256SUMS` still describes the files
    beside it and that `SHA256SUMS.minisig` still verifies it. If the prune rewrote `SHA256SUMS`,
    the signature is made again with `packaging/sign-sums.ps1`, the same key the release signs with.
#>

function Get-SitePruneScript {
    <#
    .SYNOPSIS
        The shared prune script, found from the site folder, which sits at `sites/<name>` in the
        sites repository.

    .PARAMETER SitePath
        The site folder, `black-rainbow-labs-sites/sites/inillucent`.
    #>
    param([string] $SitePath)
    $script = Join-Path (Split-Path -Parent $SitePath) 'scripts/prune-downloads.mjs'
    if (-not (Test-Path -LiteralPath $script)) {
        throw "$script does not exist. The sites checkout beside this repository needs the change that added the downloads prune; pull its main branch."
    }
    return $script
}

function Invoke-SiteDownloadsPrune {
    <#
    .SYNOPSIS
        Runs the shared prune on the site's downloads folder, or only prints what it would delete.

    .PARAMETER SitePath
        The site folder.

    .PARAMETER Version
        The version being released. It is the current version even before its files are staged,
        which is the case under ship.ps1 -WhatIf.

    .PARAMETER WhatIf
        Print what would be deleted and delete nothing.
    #>
    param([string] $SitePath, [string] $Version, [bool] $WhatIf)
    $script = Get-SitePruneScript -SitePath $SitePath
    $downloads = Join-Path $SitePath 'public/downloads'
    $arguments = @($script, '--site-dir', $SitePath, '--keep', (Get-SitePruneKeep -Downloads $downloads -Version $Version))
    if ($WhatIf) { $arguments += '--dry-run' }
    $sumsBefore = Get-FileText -Path (Join-Path $downloads 'SHA256SUMS')
    & node @arguments
    if ($LASTEXITCODE -ne 0) { throw "the downloads prune failed with $LASTEXITCODE" }
    if ($WhatIf) { return }
    if ((Get-FileText -Path (Join-Path $downloads 'SHA256SUMS')) -ne $sumsBefore) {
        Write-Host '   the prune rewrote SHA256SUMS, so it is signed again'
        Update-SiteSumsSignature -Downloads $downloads
    }
    $problem = Test-SiteSums -Downloads $downloads
    if ($problem) { throw $problem }
    Write-Host '   SHA256SUMS matches every file it names in public/downloads'
}

function Get-SitePruneKeep {
    <#
    .SYNOPSIS
        How many versions the prune keeps: 4, or 3 when this release is not in the folder yet.

    .DESCRIPTION
        The prune counts the versions it finds in the folder. Under `ship.ps1 -WhatIf` nothing has
        been staged, so the release being cut is not among them, and keeping 4 would show a
        preview that deletes one version fewer than the release will. Keeping 3 of what is there
        leaves room for the one about to be staged, so the preview and the release agree.

    .PARAMETER Downloads
        The site's `public/downloads` folder.

    .PARAMETER Version
        The version being released.
    #>
    param([string] $Downloads, [string] $Version)
    $staged = @(Get-ChildItem -LiteralPath $Downloads -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match "(^|[-_.])$([regex]::Escape($Version))([-_.]|$)" })
    if ($staged.Count -gt 0) { return '4' }
    return '3'
}

function Get-FileText {
    <#
    .SYNOPSIS
        A file's text, or an empty string when it does not exist.

    .PARAMETER Path
        The file.
    #>
    param([string] $Path)
    if (-not (Test-Path -LiteralPath $Path)) { return '' }
    return [System.IO.File]::ReadAllText($Path)
}

function Test-SiteSums {
    <#
    .SYNOPSIS
        Says what is wrong with the downloads folder's SHA256SUMS, or $null when every file it names
        is present with that hash.

    .DESCRIPTION
        install.sh and install.ps1 download a file and check it against this list, so a line whose
        file the prune removed would make an install fail. The check reads the file on disk rather
        than trusting that the prune left the list alone.

    .PARAMETER Downloads
        The site's `public/downloads` folder.
    #>
    param([string] $Downloads)
    $sums = Join-Path $Downloads 'SHA256SUMS'
    if (-not (Test-Path -LiteralPath $sums)) { return "$sums does not exist" }
    foreach ($line in Get-Content -LiteralPath $sums) {
        if (-not $line.Trim()) { continue }
        $parts = $line.Trim() -split '\s+', 2
        $name = $parts[1].TrimStart('*').Trim()
        $path = Join-Path $Downloads $name
        if (-not (Test-Path -LiteralPath $path)) { return "SHA256SUMS names $name, and public/downloads does not have it" }
        $hash = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLower()
        if ($hash -ne $parts[0].ToLower()) { return "$name does not match its SHA256SUMS line" }
    }
    return $null
}

function Update-SiteSumsSignature {
    <#
    .SYNOPSIS
        Signs the site's SHA256SUMS again after the prune rewrote it.

    .DESCRIPTION
        `sign-sums.ps1` signs `dist/SHA256SUMS` and checks the result against
        `packaging/inillucent.pub`, so the rewritten list goes into dist/ first. dist/ then
        describes what the site serves, which is what it is for.

    .PARAMETER Downloads
        The site's `public/downloads` folder.
    #>
    param([string] $Downloads)
    $packaging = Split-Path -Parent $PSScriptRoot
    $dist = Join-Path (Split-Path -Parent $packaging) 'dist'
    New-Item -ItemType Directory -Force -Path $dist | Out-Null
    Copy-Item -LiteralPath (Join-Path $Downloads 'SHA256SUMS') -Destination $dist -Force
    & (Join-Path $packaging 'sign-sums.ps1')
    Copy-Item -LiteralPath (Join-Path $dist 'SHA256SUMS.minisig') -Destination $Downloads -Force
}
