use std::{env, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-changed=assets/lume.ico");
    println!("cargo:rerun-if-changed=assets/lume-cbz.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let icon = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("assets/lume.ico");
    let comic_icon =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("assets/lume-cbz.ico");
    let rc = out.join("lume.rc");
    std::fs::write(
        &rc,
        format!(
            "1 ICON \"{}\"\n2 ICON \"{}\"\n",
            icon.display().to_string().replace('\\', "/"),
            comic_icon.display().to_string().replace('\\', "/")
        ),
    )
    .unwrap();
    let gnu = env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu");
    let obj = out.join(if gnu { "icon.o" } else { "icon.res" });
    let status = if gnu {
        Command::new("windres").arg(&rc).arg(&obj).status()
    } else {
        Command::new("rc.exe")
            .arg("/nologo")
            .arg(format!("/fo{}", obj.display()))
            .arg(&rc)
            .status()
    }
    .expect("Windows resource compiler required");
    assert!(status.success());
    println!("cargo:rustc-link-arg={}", obj.display());
}
