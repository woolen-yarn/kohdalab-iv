#![forbid(unsafe_code)]

fn main() {
    let result = (|| -> Result<i32, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let gui = exe
            .parent()
            .ok_or("Missing application folder")?
            .join(if cfg!(windows) {
                "kohdalab-iv-next.exe"
            } else {
                "kohdalab-iv-next"
            });
        let mut arguments: Vec<_> = std::env::args_os().skip(1).collect();
        if arguments.is_empty() {
            arguments.push("--version".into());
        }
        let status = std::process::Command::new(gui)
            .args(arguments)
            .status()
            .map_err(|e| e.to_string())?;
        Ok(status.code().unwrap_or(1))
    })();
    match result {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
