$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Windows.Forms
$root=[IO.Path]::GetFullPath((Split-Path $PSScriptRoot -Parent))
$expected=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'Programs\LumeView'))
if($root -ne $expected){[Windows.Forms.MessageBox]::Show('Local de instalação inválido. Nenhum arquivo foi removido.','LumeView');exit 1}
if([Windows.Forms.MessageBox]::Show('Desinstalar o LumeView? Suas imagens e preferências serão preservadas.','LumeView','YesNo') -ne 'Yes'){exit 0}
if(Get-Process lumeview -ErrorAction SilentlyContinue | Where-Object {$_.Path -eq (Join-Path $root 'lumeview.exe')}){[Windows.Forms.MessageBox]::Show('Feche o LumeView antes de desinstalar.','LumeView');exit 1}
foreach($ext in @('.jpg','.jpeg','.png','.webp','.avif','.gif','.bmp','.tif','.tiff','.ico','.heic','.cbz')){Remove-ItemProperty -LiteralPath "HKCU:\Software\Classes\$ext\OpenWithProgids" -Name 'LumeView.Image','LumeView.CBZ' -ErrorAction SilentlyContinue}
$fileIcon='HKCU:\Software\Classes\.cbz\DefaultIcon'
if(Test-Path -LiteralPath $fileIcon){
 $registeredIcon=(Get-Item -LiteralPath $fileIcon).GetValue('')
 if($registeredIcon -eq ('"'+(Join-Path $root 'assets\lumeview-cbz.ico')+'",0')){Remove-Item -LiteralPath $fileIcon -Force}
}
Remove-ItemProperty -LiteralPath 'HKCU:\Software\RegisteredApplications' -Name LumeView -ErrorAction SilentlyContinue
foreach($key in @('HKCU:\Software\Classes\LumeView.CBZ','HKCU:\Software\Classes\LumeView.Image','HKCU:\Software\Classes\Applications\lumeview.exe','HKCU:\Software\LumeView\Capabilities','HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\LumeView')){if(Test-Path -LiteralPath $key){Remove-Item -LiteralPath $key -Recurse -Force}}
$shortcut=Join-Path ([Environment]::GetFolderPath('Programs')) 'LumeView.lnk'
if(Test-Path -LiteralPath $shortcut){Remove-Item -LiteralPath $shortcut -Force}
# Both absolute paths were validated before recursive deletion.
Remove-Item -LiteralPath $root -Recurse -Force
