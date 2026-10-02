<#
.SYNOPSIS
    The install page is rewritten for each release, and a page that names another release is refused.

.DESCRIPTION
    inillucent.com/install.md named 1.0.29 while the site served 2.0.3, because nothing rewrote it on
    a release. `packaging/site/update-install-page.mjs` now rewrites it beside the download list,
    and `Test-InstallPage` is what `ship.ps1` asks of the live page. The fixture is the stale page
    exactly as it was served.

    Written for Pester 3.4, the version Windows ships:

        Invoke-Pester packaging/tests
#>

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$packaging = Split-Path -Parent $here
. (Join-Path $packaging 'site/install-page.ps1')

$stale = Join-Path $here 'fixtures/install-1.0.29.fixture'
$entries = @(
    [pscustomobject]@{ platform = 'Windows'; detail = 'x86-64 · 22.3 MB'; href = '/downloads/inillucent-2.0.4-x86_64-pc-windows-msvc.zip'; sha256 = ('a' * 64) },
    [pscustomobject]@{ platform = 'macOS'; detail = 'Apple silicon and Intel, signed and notarised · 45 MB'; href = '/downloads/inillucent-2.0.4.pkg'; sha256 = ('b' * 64) }
)

Describe 'Test-InstallPage' {
    It 'refuses the page that was served with 2.0.3' {
        $text = Get-Content -LiteralPath $stale -Raw -Encoding UTF8
        Test-InstallPage -Text $text -Version '2.0.3' | Should Be 'inillucent.com/install.md does not say it installs 2.0.3'
    }
}

Describe 'update-install-page.mjs' {
    It 'rewrites every version and the archive list, and the result passes' {
        $page = Join-Path $env:TEMP "install-page-test-$PID.md"
        Copy-Item -LiteralPath $stale -Destination $page -Force
        $json = $entries | ConvertTo-Json -Depth 4 -Compress
        & node (Join-Path $packaging 'site/update-install-page.mjs') $page $json '2.0.4' | Out-Null
        $LASTEXITCODE | Should Be 0
        $text = Get-Content -LiteralPath $page -Raw -Encoding UTF8
        Test-InstallPage -Text $text -Version '2.0.4' | Should BeNullOrEmpty
        $text | Should Match 'You are installing \*\*Inillucent 2\.0\.4\*\*'
        $text | Should Match 'must print `inillucent 2\.0\.4`'
        $text | Should Match ([regex]::Escape('- **Windows** — `inillucent-2.0.4-x86_64-pc-windows-msvc.zip` (x86-64 · 22.3 MB), SHA-256 `' + ('a' * 64) + '`'))
        $text | Should Not Match '1\.0\.29'
        $text | Should Match 'Both install scripts check the archive'
        Remove-Item -LiteralPath $page -Force -Confirm:$false
    }

    It 'refuses a page that has lost the line naming its version' {
        $page = Join-Path $env:TEMP "install-page-broken-$PID.md"
        Set-Content -LiteralPath $page -Value "# Install`n`nNothing here names a version." -Encoding UTF8
        $json = $entries | ConvertTo-Json -Depth 4 -Compress
        & node (Join-Path $packaging 'site/update-install-page.mjs') $page $json '2.0.4' 2>$null | Out-Null
        $LASTEXITCODE | Should Not Be 0
        Remove-Item -LiteralPath $page -Force -Confirm:$false
    }
}
