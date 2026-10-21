#![forbid(unsafe_code)]

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let icon = manifest.join("../../assets/kohdalab-iv.ico");
    let template = manifest.join("../../../native/windows_version.rc");
    println!("cargo:rerun-if-changed={}", icon.display());
    println!("cargo:rerun-if-changed={}", template.display());
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
    let version = env::var("CARGO_PKG_VERSION").expect("Cargo package version is missing");
    let version_words = windows_version_words(&version);
    let template =
        fs::read_to_string(&template).expect("Windows version resource template is missing");
    let resource_text = render_version_resource(
        &template,
        &version,
        &version_words,
        "KohdaLab IV",
        "kohdalab-iv",
        "kohdalab-iv.exe",
    );
    fs::write(
        &resource,
        format!("#pragma code_page(65001)\n1 ICON \"{filename}\"\n\n{resource_text}"),
    )
    .expect("write Windows icon resource");
    let msvc = env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let object = out.join(if msvc {
        "application.res"
    } else {
        "application.o"
    });
    compile_resource(&resource, &object, msvc);
    println!("cargo:rustc-link-arg-bin=kohdalab-iv={}", object.display());
    let cli_resource = out.join("iv-cli.rc");
    let cli_text = render_version_resource(
        &template,
        &version,
        &version_words,
        "KohdaLab IV Command Line",
        "iv-cli",
        "iv-cli.exe",
    );
    fs::write(
        &cli_resource,
        format!("#pragma code_page(65001)\n1 ICON \"{filename}\"\n\n{cli_text}"),
    )
    .expect("write Windows CLI version resource");
    let cli_object = out.join(if msvc { "iv-cli.res" } else { "iv-cli.o" });
    compile_resource(&cli_resource, &cli_object, msvc);
    println!("cargo:rustc-link-arg-bin=iv-cli={}", cli_object.display());
}

fn compile_resource(resource: &Path, object: &Path, msvc: bool) {
    let mut compiler = if msvc {
        let mut command = Command::new(env::var("RC").unwrap_or_else(|_| "rc.exe".into()));
        command.arg("/nologo").arg("/fo").arg(object).arg(resource);
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
            .arg(resource)
            .arg("-o")
            .arg(object);
        command
    };
    let status = compiler.status().expect("Windows resource compiler missing; use MinGW windres or Windows SDK rc.exe (WINDRES/RC can override)");
    assert!(
        status.success(),
        "Windows application resource compilation failed"
    );
}

fn render_version_resource(
    template: &str,
    version: &str,
    version_words: &str,
    description: &str,
    internal_name: &str,
    original_filename: &str,
) -> String {
    let resource = template
        .replace("@FILEVERSION_COMMAS@", version_words)
        .replace("@PRODUCTVERSION_COMMAS@", version_words)
        .replace("@COMPANY_NAME@", "Kohdalab")
        .replace("@FILE_DESCRIPTION@", description)
        .replace("@FILE_VERSION@", &rc_string(version))
        .replace("@INTERNAL_NAME@", internal_name)
        .replace("@ORIGINAL_FILENAME@", original_filename)
        .replace("@PRODUCT_NAME@", "KohdaLab IV")
        .replace("@PRODUCT_VERSION@", &rc_string(version));
    assert!(
        !resource.contains('@'),
        "unexpanded placeholder in Windows version resource template"
    );
    resource
}

fn rc_string(value: &str) -> String {
    assert!(
        !value.chars().any(|character| character.is_control()),
        "Windows resource strings must not contain control characters"
    );
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn windows_version_words(version: &str) -> String {
    let core = version.split_once('+').map_or(version, |(core, _)| core);
    let (release, pre) = core
        .split_once('-')
        .map_or((core, None), |(release, pre)| (release, Some(pre)));
    let mut components = release.split('.').map(|part| {
        part.parse::<u16>()
            .expect("Cargo package version must contain u16 components")
    });
    let mut values = [
        components
            .next()
            .expect("Cargo package version must have major component"),
        components
            .next()
            .expect("Cargo package version must have minor component"),
        components
            .next()
            .expect("Cargo package version must have patch component"),
        0,
    ];
    assert!(
        components.next().is_none(),
        "Cargo package version has too many components"
    );
    if let Some(pre) = pre {
        let mut pre = pre.split('.');
        let label = pre.next().unwrap_or_default();
        let number = pre.next().and_then(|part| part.parse::<u16>().ok());
        values[3] = match (label, number, pre.next()) {
            ("alpha", Some(number), None) => 1000u16
                .checked_add(number)
                .expect("Cargo alpha revision is too large for Windows VERSIONINFO"),
            ("beta", Some(number), None) => 2000u16
                .checked_add(number)
                .expect("Cargo beta revision is too large for Windows VERSIONINFO"),
            ("rc", Some(number), None) => 3000u16
                .checked_add(number)
                .expect("Cargo rc revision is too large for Windows VERSIONINFO"),
            _ => 1,
        };
    }
    values.map(|value| value.to_string()).join(",")
}
