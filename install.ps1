# LumeView Windows download/install helper. Review before running.
# Downloads a fixed release and checks its SHA-256 before executing anything.
# No elevation, execution-policy changes or security-setting changes.
[CmdletBinding()]
param([switch]$DownloadOnly)

$ErrorActionPreference = 'Stop'
$releaseVersion = '0.33.0'
$releaseUrl = 'https://github.com/flavw/LumeView/releases/download/v0.33.0/LumeView-Setup.exe'
$expectedSha256 = '3BEDDEBC9239D2FD7FBBE6D54EE71D31603994D72D02791FF1A4743464602A18'
$downloadDirectory = $null
$retainDownload = $false

try {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT -or -not [Environment]::Is64BitOperatingSystem) {
        throw 'LumeView requires 64-bit Windows.'
    }
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
    $downloadDirectory = [IO.Path]::GetFullPath((Join-Path $tempRoot ('LumeView-download-' + [Guid]::NewGuid().ToString('N'))))
    New-Item -ItemType Directory -Path $downloadDirectory -ErrorAction Stop | Out-Null
    $installerPath = Join-Path $downloadDirectory 'LumeView-Setup.exe'
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    Write-Host "Downloading LumeView $releaseVersion..."
    Invoke-WebRequest -UseBasicParsing -Uri $releaseUrl -OutFile $installerPath
    $actualSha256 = (Get-FileHash -LiteralPath $installerPath -Algorithm SHA256).Hash
    if ($actualSha256 -ne $expectedSha256) {
        throw 'SHA-256 mismatch. The downloaded installer will not be executed.'
    }
    Write-Host 'SHA-256 verified.'
    if ($DownloadOnly) {
        $retainDownload = $true
        Write-Host "Download only: nothing was installed. Installer: $installerPath"
    } else {
        Write-Host 'Installing for the current Windows user. Close LumeView before updating.'
        $process = Start-Process -FilePath $installerPath -ArgumentList '/S' -WindowStyle Hidden -Wait -PassThru
        if ($process.ExitCode -ne 0) {
            $reason = switch ($process.ExitCode) {
                1001 { 'Close LumeView and try again.' }
                1618 { 'Another LumeView installation is already running.' }
                112 { 'Not enough disk space.' }
                87 { 'Invalid installer parameters.' }
                default { 'See https://flavw.github.io/LumeView/installer-errors.html' }
            }
            throw "Installer returned $($process.ExitCode). $reason"
        }
        Write-Host 'LumeView installed successfully. Find it in the Start menu.'
    }
} finally {
    if ($downloadDirectory -and -not $retainDownload -and (Test-Path -LiteralPath $downloadDirectory)) {
        # Delete only the exact temporary directory created by this invocation.
        $resolvedDirectory = [IO.Path]::GetFullPath($downloadDirectory)
        $resolvedRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd([IO.Path]::DirectorySeparatorChar)
        if (-not $resolvedDirectory.StartsWith($resolvedRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
            [IO.Path]::GetFileName($resolvedDirectory) -notmatch '^LumeView-download-[a-f0-9]{32}$') {
            throw 'Refusing cleanup outside the expected temporary directory.'
        }
        $entry = Get-Item -LiteralPath $resolvedDirectory -Force
        if ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Refusing cleanup of a linked directory.' }
        Remove-Item -LiteralPath (Join-Path $resolvedDirectory 'LumeView-Setup.exe') -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $resolvedDirectory -ErrorAction SilentlyContinue
    }
}
