#![cfg_attr(windows, windows_subsystem = "windows")]

#[derive(Default)]
struct Options {
    silent: bool,
    stage_only: bool,
    target: Option<String>,
    language: usize,
}
impl Options {
    fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut options = Self::default();
        for arg in args {
            match arg.to_ascii_lowercase().as_str() {
                "/s" | "/silent" | "--silent" => options.silent = true,
                "--stage-only" => options.stage_only = true,
                _ if arg.starts_with("--target=") => {
                    let target = arg[9..].to_owned();
                    if target.is_empty() { return Err("Empty target directory.".into()); }
                    options.target = Some(target);
                }
                _ if arg.starts_with("--lang=") => {
                    options.language = match &arg[7..] {
                        "en" => 0, "pt" => 1, "es" => 2, "fr" => 3,
                        _ => return Err("Supported languages: en, pt, es, fr.".into()),
                    };
                }
                _ => return Err(format!("Unknown argument: {arg}")),
            }
        }
        if options.stage_only && options.target.is_none() {
            return Err("--stage-only requires --target=<directory>.".into());
        }
        Ok(options)
    }
}

#[cfg(windows)]
mod installer {
    use super::Options;
    use std::{hash::{Hash, Hasher}, os::windows::process::CommandExt, process::Command};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_ABANDONED},
        System::Threading::{CreateMutexW, WaitForSingleObject, ReleaseMutex},
        UI::WindowsAndMessaging::{MessageBoxW, IDYES, MB_ICONERROR, MB_ICONINFORMATION, MB_YESNO},
    };
    fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(Some(0)).collect() }
    fn message(text: &str, flags: u32) -> i32 {
        unsafe { MessageBoxW(std::ptr::null_mut(), wide(text).as_ptr(), wide("LumeView Setup").as_ptr(), flags) }
    }
    struct Lock(HANDLE);
    impl Drop for Lock {
        fn drop(&mut self) { unsafe { ReleaseMutex(self.0); CloseHandle(self.0); } }
    }
    impl Lock {
        fn acquire() -> Result<Self, (i32, String)> {
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            std::env::var_os("LOCALAPPDATA").hash(&mut hash);
            let name = wide(&format!("Local\\LumeViewSetup-{:016x}", hash.finish()));
            let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
            if handle.is_null() { return Err((1603, std::io::Error::last_os_error().to_string())); }
            let result = unsafe { WaitForSingleObject(handle, 0) };
            if result == WAIT_OBJECT_0 || result == WAIT_ABANDONED { return Ok(Self(handle)); }
            unsafe { CloseHandle(handle); }
            Err((1618, "Another LumeView installation is in progress.".into()))
        }
    }
    fn io_error(e: std::io::Error) -> (i32, String) {
        let code = if matches!(e.raw_os_error(), Some(112 | 39)) {112} else {1603};
        (code, e.to_string())
    }
    fn install(options: &Options) -> Result<(), (i32, String)> {
        let _lock = Lock::acquire()?;
        let temp = tempfile::Builder::new().prefix("lumeview-setup-").tempdir().map_err(io_error)?;
        let zip = temp.path().join("payload.zip");
        let script = temp.path().join("install.ps1");
        std::fs::write(&zip, include_bytes!(env!("LUMEVIEW_INSTALLER_PAYLOAD"))).map_err(io_error)?;
        std::fs::write(&script, include_bytes!("../../installer/install.ps1")).map_err(io_error)?;
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(script).arg("-Payload").arg(zip).creation_flags(0x08000000);
        if let Some(target) = &options.target { command.arg("-Target").arg(target); }
        if options.stage_only { command.arg("-StageOnly"); }
        let output = command.output().map_err(io_error)?;
        if !output.status.success() {
            return Err((output.status.code().unwrap_or(1603), String::from_utf8_lossy(&output.stderr).chars().take(1800).collect()));
        }
        Ok(())
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn concurrent_installation_and_disk_errors() {
            let lock = Lock::acquire().unwrap_or_else(|e| panic!("{}", e.1));
            let code = std::thread::spawn(|| match Lock::acquire() { Ok(_) => 0, Err((code, _)) => code }).join().unwrap();
            assert_eq!(code, 1618);
            drop(lock);
            assert_eq!(io_error(std::io::Error::from_raw_os_error(112)).0, 112);
            assert_eq!(io_error(std::io::Error::from_raw_os_error(39)).0, 112);
            assert_eq!(io_error(std::io::Error::from_raw_os_error(5)).0, 1603);
        }
    }
    pub fn run() -> i32 {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let silent_requested = args.iter().any(|a| matches!(a.to_ascii_lowercase().as_str(), "/s" | "/silent" | "--silent"));
        let options = match Options::parse(args.into_iter()) {
            Ok(options) => options,
            Err(error) => { if !silent_requested { message(&error, MB_ICONERROR); } return 87; }
        };
        let prompts = [
            "Install LumeView for this user?\n\nCreates a shortcut and registers Open with. Your default apps will not be changed.",
            "Instalar o LumeView para este usuário?\n\nCria um atalho e registra Abrir com. Seus aplicativos padrão não serão alterados.",
            "¿Instalar LumeView para este usuario?\n\nCrea un acceso directo y registra Abrir con. No cambia las aplicaciones predeterminadas.",
            "Installer LumeView pour cet utilisateur ?\n\nCrée un raccourci et ajoute Ouvrir avec. Les applications par défaut restent inchangées.",
        ];
        if !options.silent && message(prompts[options.language], MB_YESNO | MB_ICONINFORMATION) != IDYES { return 1602; }
        match install(&options) {
            Ok(()) => {
                if !options.silent {
                    let completed = ["LumeView installed successfully.", "LumeView instalado com sucesso.", "LumeView instalado correctamente.", "LumeView installé avec succès."];
                    message(completed[options.language], MB_ICONINFORMATION);
                }
                0
            }
            Err((code, error)) => {
                if !options.silent {
                    let failed = ["Installation failed", "Falha na instalação", "Error de instalación", "Échec de l’installation"];
                    message(&format!("{} ({code}):\n{error}", failed[options.language]), MB_ICONERROR);
                }
                code
            }
        }
    }
}
#[cfg(windows)]
fn main() { std::process::exit(installer::run()); }
#[cfg(not(windows))]
fn main() { eprintln!("This installer is for Windows only."); std::process::exit(1633); }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn silent_flags_and_language_are_validated() {
        for flag in ["/S", "/silent", "--silent"] {
            let options = Options::parse([flag.into(), "--lang=fr".into()].into_iter()).unwrap();
            assert!(options.silent); assert_eq!(options.language, 3);
        }
        assert!(Options::parse(["--lang=xx".into()].into_iter()).is_err());
        assert!(Options::parse(["--stage-only".into()].into_iter()).is_err());
        assert!(Options::parse(["/invalid".into()].into_iter()).is_err());
        let options = Options::parse(["--stage-only".into(), "--target=C:\\test folder".into()].into_iter()).unwrap();
        assert!(options.stage_only); assert_eq!(options.target.as_deref(), Some("C:\\test folder"));
    }
}
