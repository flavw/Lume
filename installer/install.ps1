param([Parameter(Mandatory=$true)][string]$Payload,[string]$Target=(Join-Path $env:LOCALAPPDATA 'Programs\LumeView'),[switch]$StageOnly)
$ErrorActionPreference='Stop'
$Target=[IO.Path]::GetFullPath($Target)
$exe=Join-Path $Target 'lumeview.exe'
if(Get-Process lumeview -ErrorAction SilentlyContinue | Where-Object {$_.Path -eq $exe}){throw 'Feche o LumeView antes de instalar ou atualizar.'}
New-Item -ItemType Directory -Force -Path $Target | Out-Null
Expand-Archive -LiteralPath $Payload -DestinationPath $Target -Force
$toolsRoot=[IO.Path]::GetFullPath((Join-Path $Target 'tools'))
if(Test-Path -LiteralPath $toolsRoot){
 foreach($folder in Get-ChildItem -LiteralPath $toolsRoot -Directory){
  $resolved=[IO.Path]::GetFullPath($folder.FullName)
  if($folder.Name -notin @('imagemagick','jpegtran') -and $resolved.StartsWith($toolsRoot+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){Remove-Item -LiteralPath $resolved -Recurse -Force}
 }
}
if($StageOnly){Write-Output "Arquivos preparados em $Target";exit 0}
$extensions=@('.jpg','.jpeg','.png','.webp','.avif','.gif','.bmp','.tif','.tiff','.ico','.heic','.cbz')
$prog='HKCU:\Software\Classes\LumeView.Image'
New-Item -Force -Path $prog | Out-Null
Set-Item -LiteralPath $prog -Value 'Imagem ou quadrinho do LumeView'
New-Item -Force -Path "$prog\DefaultIcon" | Out-Null
Set-Item -LiteralPath "$prog\DefaultIcon" -Value ('"'+$exe+'",0')
New-Item -Force -Path "$prog\shell\open\command" | Out-Null
Set-Item -LiteralPath "$prog\shell\open\command" -Value ('"'+$exe+'" "%1"')
$comic='HKCU:\Software\Classes\LumeView.CBZ'
New-Item -Force -Path $comic,"$comic\DefaultIcon","$comic\shell\open\command" | Out-Null
Set-Item -LiteralPath $comic -Value 'Quadrinho CBZ do LumeView'
Set-Item -LiteralPath "$comic\DefaultIcon" -Value ('"'+(Join-Path $Target 'assets\lumeview-cbz.ico')+'",0')
New-Item -Force -Path 'HKCU:\Software\Classes\.cbz\DefaultIcon' | Out-Null
Set-Item -LiteralPath 'HKCU:\Software\Classes\.cbz\DefaultIcon' -Value ('"'+(Join-Path $Target 'assets\lumeview-cbz.ico')+'",0')
Set-Item -LiteralPath "$comic\shell\open\command" -Value ('"'+$exe+'" "%1"')
$app='HKCU:\Software\Classes\Applications\lumeview.exe'
New-Item -Force -Path "$app\shell\open\command","$app\SupportedTypes" | Out-Null
Set-ItemProperty -LiteralPath $app -Name FriendlyAppName -Value 'LumeView'
Set-Item -LiteralPath "$app\shell\open\command" -Value ('"'+$exe+'" "%1"')
$cap='HKCU:\Software\LumeView\Capabilities'
New-Item -Force -Path "$cap\FileAssociations" | Out-Null
Set-ItemProperty -LiteralPath $cap -Name ApplicationName -Value 'LumeView'
Set-ItemProperty -LiteralPath $cap -Name ApplicationDescription -Value 'Visualizador de imagens e quadrinhos leve e open source.'
Set-ItemProperty -LiteralPath $cap -Name ApplicationIcon -Value ('"'+$exe+'",0')
foreach($oldExt in (Get-Item -LiteralPath "$app\SupportedTypes").GetValueNames()){
 if($oldExt.StartsWith('.') -and $oldExt -notin $extensions){
  Remove-ItemProperty -LiteralPath "HKCU:\Software\Classes\$oldExt\OpenWithProgids" -Name 'LumeView.Image' -ErrorAction SilentlyContinue
  Remove-ItemProperty -LiteralPath "$app\SupportedTypes" -Name $oldExt -ErrorAction SilentlyContinue
  Remove-ItemProperty -LiteralPath "$cap\FileAssociations" -Name $oldExt -ErrorAction SilentlyContinue
 }
}
foreach($ext in $extensions){
 $association=if($ext -eq '.cbz'){'LumeView.CBZ'}else{'LumeView.Image'}
 if($ext -eq '.cbz'){Remove-ItemProperty -LiteralPath "HKCU:\Software\Classes\$ext\OpenWithProgids" -Name 'LumeView.Image' -ErrorAction SilentlyContinue}
 New-Item -Force -Path "HKCU:\Software\Classes\$ext\OpenWithProgids" | Out-Null
 New-ItemProperty -Path "HKCU:\Software\Classes\$ext\OpenWithProgids" -Name $association -PropertyType String -Value '' -Force | Out-Null
 New-ItemProperty -Path "$app\SupportedTypes" -Name $ext -PropertyType String -Value '' -Force | Out-Null
 Set-ItemProperty -LiteralPath "$cap\FileAssociations" -Name $ext -Value $association
}
New-Item -Force -Path 'HKCU:\Software\RegisteredApplications' | Out-Null
Set-ItemProperty -LiteralPath 'HKCU:\Software\RegisteredApplications' -Name LumeView -Value 'Software\LumeView\Capabilities'
$start=Join-Path ([Environment]::GetFolderPath('Programs')) 'LumeView.lnk'
$shell=New-Object -ComObject WScript.Shell
$link=$shell.CreateShortcut($start);$link.TargetPath=$exe;$link.WorkingDirectory=$Target;$link.IconLocation="$exe,0";$link.Save()
$uninstall='HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\LumeView'
New-Item -Force -Path $uninstall | Out-Null
Set-ItemProperty -LiteralPath $uninstall -Name DisplayName -Value 'LumeView'
Set-ItemProperty -LiteralPath $uninstall -Name DisplayVersion -Value '0.27.0'
Set-ItemProperty -LiteralPath $uninstall -Name DisplayIcon -Value $exe
Set-ItemProperty -LiteralPath $uninstall -Name InstallLocation -Value $Target
Set-ItemProperty -LiteralPath $uninstall -Name UninstallString -Value ('powershell.exe -NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File "'+(Join-Path $Target 'installer\uninstall.ps1')+'"')
Set-ItemProperty -LiteralPath $uninstall -Name NoModify -Type DWord -Value 1
Set-ItemProperty -LiteralPath $uninstall -Name NoRepair -Type DWord -Value 1
Write-Output 'LumeView instalado. Para defini-lo como padrão, abra Configurações > Aplicativos > Aplicativos padrão e procure LumeView.'
