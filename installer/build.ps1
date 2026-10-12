$ErrorActionPreference='Stop'
$projectRoot=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$buildRoot=Join-Path $projectRoot 'build'
New-Item -ItemType Directory -Force -Path $buildRoot | Out-Null
$releaseRoot=if($env:CARGO_TARGET_DIR){Join-Path ([IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)) 'release'}else{Join-Path $projectRoot 'target\release'}
$releaseExe=Join-Path $releaseRoot 'lumeview.exe'
if(!(Test-Path -LiteralPath $releaseExe)){throw 'Build the release app first: cargo build --release --bin lumeview'}
Copy-Item -LiteralPath $releaseExe -Destination (Join-Path $projectRoot 'lumeview.exe') -Force
$payload=Join-Path $buildRoot 'installer-payload.zip'
$items=@('lumeview.exe','LICENSE','README.md','PRIVACY.md','THIRD_PARTY_LICENSES.txt','Cargo.toml','Cargo.lock','build.rs','tools','assets','src','installer','tests','docs') | ForEach-Object {Join-Path $projectRoot $_}
Compress-Archive -LiteralPath $items -DestinationPath $payload -Force
$env:LUMEVIEW_INSTALLER_PAYLOAD=$payload
cargo build --release --manifest-path (Join-Path $projectRoot 'Cargo.toml') --features installer --bin lumeview-setup
if($LASTEXITCODE -ne 0){throw 'Installer build failed'}
Copy-Item -LiteralPath (Join-Path $releaseRoot 'lumeview-setup.exe') -Destination (Join-Path $projectRoot 'LumeView-Setup.exe') -Force
