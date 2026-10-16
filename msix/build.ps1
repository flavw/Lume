param([string]$SdkBin)
$ErrorActionPreference='Stop'
$root=Split-Path $PSScriptRoot
if(!$SdkBin){$SdkBin=Join-Path $root '.dev/msix-sdk/bin/10.0.28000.0/x64'}
$makeappx=Join-Path $SdkBin 'makeappx.exe'
if(!(Test-Path $makeappx)){throw 'Pass -SdkBin pointing to a Windows SDK x64 bin directory.'}
$staging=Join-Path $PSScriptRoot 'staging'
$release=Join-Path $root 'target/release/lumeview.exe'
if(!(Test-Path $release)){throw 'Build the release executable first.'}
New-Item -ItemType Directory -Force $staging,(Join-Path $root 'build') | Out-Null
Copy-Item -LiteralPath $release -Destination $staging -Force
foreach($name in @('tools','LICENSE','PRIVACY.md','THIRD_PARTY_LICENSES.txt')){
    Copy-Item -LiteralPath (Join-Path $root $name) -Destination $staging -Recurse -Force
}
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'Assets'),(Join-Path $PSScriptRoot 'AppxManifest.xml') -Destination $staging -Recurse -Force
[xml]$manifest=Get-Content (Join-Path $staging 'AppxManifest.xml') -Raw
$version=$manifest.Package.Identity.Version
$output=Join-Path $root "build/LumeView_${version}_x64_Store.msix"
& $makeappx pack /d $staging /p $output /o
if($LASTEXITCODE -ne 0){throw 'MSIX packaging failed'}
Write-Output "Store upload package: $output"
