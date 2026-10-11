#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
fn main() {
    use std::{os::windows::process::CommandExt, process::Command};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDYES, MB_ICONERROR, MB_ICONINFORMATION, MB_YESNO,
    };
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    fn message(s: &str, flags: u32) -> i32 {
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                wide(s).as_ptr(),
                wide("Lume — Instalador").as_ptr(),
                flags,
            )
        }
    }
    if message("Instalar o Lume para este usuário?\n\nCria um atalho e registra o programa em Abrir com. Não altera automaticamente seu aplicativo padrão.",MB_YESNO|MB_ICONINFORMATION)!=IDYES{return;}
    let result = (|| -> Result<(), String> {
        let temp = tempfile::Builder::new()
            .prefix("lume-setup-")
            .tempdir()
            .map_err(|e| e.to_string())?;
        let zip = temp.path().join("payload.zip");
        let script = temp.path().join("install.ps1");
        std::fs::write(&zip, include_bytes!(env!("LUME_INSTALLER_PAYLOAD")))
            .map_err(|e| e.to_string())?;
        std::fs::write(&script, include_bytes!("../../installer/install.ps1"))
            .map_err(|e| e.to_string())?;
        let output = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(&script)
            .arg("-Payload")
            .arg(&zip)
            .creation_flags(0x08000000)
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(1800)
                .collect());
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            message("Lume instalado!\n\nAbra Configurações > Aplicativos > Aplicativos padrão e procure Lume para escolhê-lo como padrão.",MB_ICONINFORMATION);
        }
        Err(e) => {
            message(&format!("A instalação falhou:\n{e}"), MB_ICONERROR);
        }
    }
}
#[cfg(not(windows))]
fn main() {
    eprintln!("This installer is for Windows only.");
}
