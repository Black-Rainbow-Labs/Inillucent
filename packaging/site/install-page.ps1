<#
.SYNOPSIS
    The check a release makes of the install page inillucent.com/install.md.

.DESCRIPTION
    Dot sourced by ship.ps1, whose Test-SiteVersion fetches the live page, and by
    packaging/tests/install-page.Tests.ps1, which checks it without the network.
#>

function Test-InstallPage {
    <#
    .SYNOPSIS
        Says what is wrong with an install page for a release, or nothing when it is right.

    .DESCRIPTION
        The page must name the release in its first line, in the version step 3 tells the reader to
        expect, and in every archive it lists, and it must name no other release. A separate
        function so packaging/tests can check it without the network.

    .PARAMETER Text
        The page, as served.

    .PARAMETER Version
        The version being released.
    #>
    param([string] $Text, [string] $Version)
    if (-not $Text.Contains("You are installing **Inillucent $Version**,")) {
        return "inillucent.com/install.md does not say it installs $Version"
    }
    # Contains rather than -like, because a backtick is the wildcard escape character in -like.
    if (-not $Text.Contains("must print ``inillucent $Version``")) {
        return "inillucent.com/install.md does not tell the reader to expect inillucent $Version"
    }
    $other = @([regex]::Matches($Text, '(?i)inillucent[-_ ](\d+\.\d+\.\d+)') |
        ForEach-Object { $_.Groups[1].Value } | Where-Object { $_ -ne $Version } | Sort-Object -Unique)
    if ($other.Count -gt 0) {
        return "inillucent.com/install.md still names $($other -join ', ') beside $Version"
    }
    $null
}

function Test-LlmsPage {
    <#
    .SYNOPSIS
        Says what is wrong with llms.txt for a release, or nothing when it is right.

    .DESCRIPTION
        llms.txt is the page an agent reads first, and the release rewrites nothing in it. It is
        right for every release only while it names no version and sends the reader to install.md,
        which the release does rewrite (task-2110, bug 11). So a version number in it other than the
        release's is a page that will go stale, and is refused while it is still current.

    .PARAMETER Text
        The page, as served.

    .PARAMETER Version
        The version being released.
    #>
    param([string] $Text, [string] $Version)
    if (-not $Text.Contains('https://inillucent.com/install.md')) {
        return 'inillucent.com/llms.txt does not send the reader to https://inillucent.com/install.md'
    }
    $other = @([regex]::Matches($Text, '(?<![\d.])(\d+\.\d+\.\d+)(?![\d.])') |
        ForEach-Object { $_.Groups[1].Value } | Where-Object { $_ -ne $Version } | Sort-Object -Unique)
    if ($other.Count -gt 0) {
        return "inillucent.com/llms.txt names $($other -join ', '), which no release rewrites"
    }
    $null
}
