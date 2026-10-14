#![forbid(unsafe_code)]

use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    let icon = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("../../assets/kohdalab-iv.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    println!("cargo:rerun-if-env-changed=WINDRES");
    println!("cargo:rerun-if-env-changed=RC");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let resource = out.join("application.rc");
    let icon = icon.canonicalize().expect("application icon is missing");
    let canonical = icon.to_string_lossy();
    let filename = canonical
        .strip_prefix(r"\\?\")
        .unwrap_or(&canonical)
        .replace('\\', "/")
        .replace('"', "\\\"");
    fs::write(
        &resource,
        format!("#pragma code_page(65001)\n1 ICON \"{filename}\"\n"),
    )
    .expect("write Windows icon resource");
    let msvc = env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let object = out.join(if msvc {
        "application.res"
    } else {
        "application.o"
    });
    let mut compiler = if msvc {
        let mut command = Command::new(env::var("RC").unwrap_or_else(|_| "rc.exe".into()));
        command
            .arg("/nologo")
            .arg("/fo")
            .arg(&object)
            .arg(&resource);
        command
    } else {
        let architecture = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
        let tool = env::var("WINDRES").unwrap_or_else(|_| {
            if cfg!(windows) {
                "windres".into()
            } else {
                format!("{architecture}-w64-mingw32-windres")
            }
        });
        let mut command = Command::new(tool);
        command
            .args(["-O", "coff", "-i"])
            .arg(&resource)
            .arg("-o")
            .arg(&object);
        command
    };
    let status = compiler.status().expect("Windows icon compiler missing; use MinGW windres or Windows SDK rc.exe (WINDRES/RC can override)");
    assert!(
        status.success(),
        "Windows application icon compilation failed"
    );
    for binary in ["kohdalab-iv-next", "iv-cli"] {
        println!("cargo:rustc-link-arg-bin={binary}={}", object.display());
    }
}
