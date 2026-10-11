$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Windows.Forms
$root=[IO.Path]::GetFullPath((Split-Path $PSScriptRoot -Parent))
$expected=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'Programs\Lume'))
if($root -ne $expected){[Windows.Forms.MessageBox]::Show('Local de instalação inválido. Nenhum arquivo foi removido.','Lume');exit 1}
if([Windows.Forms.MessageBox]::Show('Desinstalar o Lume? Suas imagens e preferências serão preservadas.','Lume','YesNo') -ne 'Yes'){exit 0}
if(Get-Process lume -ErrorAction SilentlyContinue | Where-Object {$_.Path -eq (Join-Path $root 'lume.exe')}){[Windows.Forms.MessageBox]::Show('Feche o Lume antes de desinstalar.','Lume');exit 1}
foreach($ext in @('.jpg','.jpeg','.png','.webp','.avif','.gif','.bmp','.tif','.tiff','.ico','.heic','.cbz')){Remove-ItemProperty -LiteralPath "HKCU:\Software\Classes\$ext\OpenWithProgids" -Name 'Lume.Image','Lume.CBZ' -ErrorAction SilentlyContinue}
$fileIcon='HKCU:\Software\Classes\.cbz\DefaultIcon'
if(Test-Path -LiteralPath $fileIcon){
 $registeredIcon=(Get-Item -LiteralPath $fileIcon).GetValue('')
 if($registeredIcon -eq ('"'+(Join-Path $root 'assets\lume-cbz.ico')+'",0')){Remove-Item -LiteralPath $fileIcon -Force}
}
Remove-ItemProperty -LiteralPath 'HKCU:\Software\RegisteredApplications' -Name Lume -ErrorAction SilentlyContinue
foreach($key in @('HKCU:\Software\Classes\Lume.CBZ','HKCU:\Software\Classes\Lume.Image','HKCU:\Software\Classes\Applications\lume.exe','HKCU:\Software\Lume\Capabilities','HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Lume')){if(Test-Path -LiteralPath $key){Remove-Item -LiteralPath $key -Recurse -Force}}
$shortcut=Join-Path ([Environment]::GetFolderPath('Programs')) 'Lume.lnk'
if(Test-Path -LiteralPath $shortcut){Remove-Item -LiteralPath $shortcut -Force}
# Both absolute paths were validated before recursive deletion.
Remove-Item -LiteralPath $root -Recurse -Force
