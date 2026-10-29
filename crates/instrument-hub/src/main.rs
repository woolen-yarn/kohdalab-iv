#![forbid(unsafe_code)]
use fs2::FileExt;
use instrument_protocol::{Client, Endpoint, PROTOCOL_VERSION, Request, SafetyPolicy};
use kohdalab_instrument_hub::{
    core::{Core, NativeBackend},
    server,
};
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, atomic::AtomicBool};
use std::time::Duration;

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if let Err(e) = execute(&args) {
        if args.iter().any(|a| a == "--client" || a == "--list") {
            kiv(false, &e);
        } else {
            eprintln!("{e}");
        }
        std::process::exit(1);
    }
}
fn option(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find(|a| a[0] == name).map(|a| a[1].clone())
}
fn required(args: &[String], name: &str) -> Result<String, String> {
    option(args, name).ok_or_else(|| format!("Missing {name}"))
}
fn execute(args: &[String]) -> Result<(), String> {
    if args.iter().any(|a| a == "--serve") {
        return serve(args);
    }
    if args.iter().any(|a| a == "--help") || args.is_empty() {
        println!(
            "KohdaLab Instrument Hub (development native GPIB backend)\n\
            --serve --native-helper PATH --state-dir PRIVATE_DIRECTORY --devices DEVICE_MAP_JSON\n\
            --client --config ENDPOINT_JSON --app NAME [--address N]\n\
            --list|--status --config ENDPOINT_JSON [--app NAME]\n\
            --recover N --model MODEL --config ENDPOINT_JSON"
        );
        return Ok(());
    }
    let endpoint = Endpoint::load(required(args, "--config")?).map_err(|e| e.to_string())?;
    let app = option(args, "--app").unwrap_or_else(|| "KohdaLab operator".into());
    let mut client = Client::connect(&endpoint, &app).map_err(|e| e.to_string())?;
    if args.iter().any(|a| a == "--list") {
        let value = request(&mut client, Request::List, 12_000)?;
        kiv(true, &value);
        let _ = request(&mut client, Request::Close, 10_000);
        return Ok(());
    }
    if args.iter().any(|a| a == "--status") {
        println!("{}", request(&mut client, Request::Status, 3_000)?);
        return Ok(());
    }
    if let Some(address) = option(args, "--recover") {
        let address = address.parse().map_err(|_| "Invalid recovery address")?;
        let model = required(args, "--model")?;
        request(&mut client, Request::Recover { address, model }, 15_000)?;
        println!("Source output verified OFF; quarantine cleared.");
        return Ok(());
    }
    if !args.iter().any(|a| a == "--client") {
        return Err("Unknown instrument hub command".into());
    }
    if let Some(address) = option(args, "--address") {
        let address = address.parse().map_err(|_| "Invalid GPIB address")?;
        request(
            &mut client,
            Request::Claim {
                address,
                policy: SafetyPolicy::Instrument,
            },
            10_000,
        )?;
    }
    kiv(true, "");
    let stdin = io::stdin();
    let mut input = stdin.lock();
    loop {
        let mut line = Vec::new();
        // Bound legacy helper input too: never allocate read_line arbitrarily.
        let count = std::io::Read::take(&mut input, 65537)
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?;
        if count == 0 {
            return Ok(());
        }
        if count > 65536 || line.last() != Some(&b'\n') {
            return Err("Oversized or truncated KIV request".into());
        }
        let text = std::str::from_utf8(&line).map_err(|_| "Non-UTF8 KIV request")?;
        let result = proxy_request(&mut client, text);
        match result {
            Ok((value, closing)) => {
                kiv(true, &value);
                if closing {
                    return Ok(());
                }
            }
            Err(e) => kiv(false, &e),
        }
    }
}

fn request(client: &mut Client, request: Request, timeout_ms: u64) -> Result<String, String> {
    client
        .request(request, Duration::from_millis(timeout_ms))
        .map_err(|e| format!("{:?}: {}", e.code, e.message))
}

fn proxy_request(client: &mut Client, text: &str) -> Result<(String, bool), String> {
    let fields: Vec<_> = text.split_whitespace().collect();
    if fields.len() != 4 {
        return Err("Invalid KIV request".into());
    }
    let operation = fields[0];
    let address: u8 = fields[1].parse().map_err(|_| "Invalid KIV address")?;
    let timeout_ms: u64 = fields[2].parse().map_err(|_| "Invalid KIV timeout")?;
    if !(1..=30).contains(&address) || !(1..=30_000).contains(&timeout_ms) {
        return Err("Invalid KIV address or timeout".into());
    }
    let payload = unhex(fields[3])?;
    let op = match operation {
        "CLAIM" => Request::Claim {
            address,
            policy: serde_json::from_slice(&payload)
                .map_err(|_| "Invalid source safety registration")?,
        },
        "RELEASE" => Request::Release { address },
        "LIST" => Request::List,
        "EXIT" => Request::Close,
        // A legacy app releases its own session. REN must stay asserted for
        // other clients; only the hardware worker's final Drop releases it.
        "REN" if payload == b"0" => return Ok((String::new(), false)),
        "WRITE" | "QUERY" | "READ" | "CLEAR" | "LOCAL" => Request::Transfer {
            operation: operation.into(),
            address,
            timeout_ms,
            payload,
        },
        _ => return Err("Unsupported or global GPIB operation".into()),
    };
    let result = request(
        client,
        op,
        timeout_ms.max(if matches!(operation, "EXIT" | "RELEASE" | "CLAIM") {
            10_000
        } else {
            1
        }),
    )?;
    Ok((result, operation == "EXIT"))
}

fn kiv(ok: bool, payload: &str) {
    let encoded = if payload.is_empty() {
        // Native helper replies use an empty hex field. '-' is accepted for
        // requests, but the IV reply decoder deliberately accepts hex only.
        String::new()
    } else {
        payload
            .bytes()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    println!("KIV\t{}\t{encoded}", if ok { "OK" } else { "ERR" });
    let _ = io::stdout().flush();
}
fn unhex(payload: &str) -> Result<Vec<u8>, String> {
    if payload == "-" {
        return Ok(Vec::new());
    }
    if !payload.len().is_multiple_of(2) || payload.len() > 32768 || !payload.is_ascii() {
        return Err("Invalid KIV payload".into());
    }
    payload
        .as_bytes()
        .chunks(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).map_err(|_| "Invalid KIV hex")?;
            u8::from_str_radix(text, 16).map_err(|_| "Invalid KIV hex".to_owned())
        })
        .collect()
}

fn private_file(path: &Path, data: &[u8]) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| e.to_string())?;
    file.write_all(data)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())
}

fn serve(args: &[String]) -> Result<(), String> {
    let helper = PathBuf::from(required(args, "--native-helper")?);
    if !helper.is_file() {
        return Err("Native GPIB helper was not found".into());
    }
    if std::env::var_os("KOHDALAB_INSTRUMENT_HUB_CONFIG").is_some() {
        return Err("Start the hardware service without the client-only KOHDALAB_INSTRUMENT_HUB_CONFIG variable".into());
    }
    let directory = PathBuf::from(required(args, "--state-dir")?);
    create_private_directory(&directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&directory)
            .map_err(|e| e.to_string())?
            .permissions()
            .mode();
        if mode & 0o077 != 0 {
            return Err("Service state directory must be private (chmod 700)".into());
        }
    }
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.join("service.lock"))
        .map_err(|e| e.to_string())?;
    lock.try_lock_exclusive()
        .map_err(|_| "An instrument service already owns this state directory")?;
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
    let endpoint = Endpoint {
        protocol: PROTOCOL_VERSION,
        address: listener.local_addr().map_err(|e| e.to_string())?,
        token: uuid::Uuid::new_v4().to_string(),
    };
    let endpoint_path = directory.join("endpoint.json");
    let devices_file = PathBuf::from(required(args, "--devices")?);
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(
        &mut std::io::Read::take(
            fs::File::open(devices_file).map_err(|e| e.to_string())?,
            8193,
        ),
        &mut bytes,
    )
    .map_err(|e| e.to_string())?;
    if bytes.len() > 8192 {
        return Err("Device map is too large".into());
    }
    let devices = serde_json::from_slice(&bytes).map_err(|e| format!("Invalid device map: {e}"))?;
    let mut core = Core::new(
        NativeBackend::new(helper),
        directory.join("source-ownership.json"),
        devices,
    )
    .map_err(|e| e.to_string())?;
    // Operator-started initialization precedes endpoint publication. Without
    // an initial passive inventory, TRKR's first lease could hide IV's other
    // addresses from Refresh (active leases deliberately suppress probing).
    eprintln!("Opening GPIB controller and caching listeners...");
    core.request(
        "service-startup",
        "Instrument service",
        Request::List,
        Duration::from_secs(30),
    )
    .map_err(|e| format!("Controller initialization failed: {e}"))?;
    // The state-directory lock fences publication. A failed initialization
    // never publishes a ready endpoint or starts client hardware fallback.
    let staged = directory.join(format!("endpoint-{}.tmp", uuid::Uuid::new_v4()));
    private_file(
        &staged,
        &serde_json::to_vec(&endpoint).map_err(|e| e.to_string())?,
    )?;
    fs::rename(&staged, &endpoint_path).map_err(|e| e.to_string())?;
    eprintln!(
        "Instrument service ready. Endpoint: {}",
        endpoint_path.display()
    );
    server::run(listener, endpoint, core, Arc::new(AtomicBool::new(false)))
}

fn create_private_directory(directory: &Path) -> Result<(), String> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(directory).map_err(|e| e.to_string())?;
    // In particular, never replace a Windows DACL on an arbitrary project or
    // data directory chosen by mistake. Only a dedicated hub state directory
    // (or a new empty directory) may have its permissions tightened.
    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let owned = matches!(
            name.as_ref(),
            "service.lock" | "endpoint.json" | "source-ownership.json" | "devices.json"
        ) || ((name.starts_with("endpoint-") || name.starts_with("source-ownership."))
            && name.ends_with(".tmp"));
        if !owned || !entry.file_type().map_err(|e| e.to_string())?.is_file() {
            return Err("Choose a dedicated instrument service state directory; unrelated files or subdirectories were found. No permissions were changed.".into());
        }
    }
    #[cfg(windows)]
    {
        // Use a new protected ACL, rather than adding a grant to an existing
        // potentially world-readable DACL. Path is an environment argument,
        // never interpolated into PowerShell source. No admin rights needed.
        let windows = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .ok_or("SystemRoot is missing; cannot protect service credentials")?;
        let powershell = windows.join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let script = r#"$ErrorActionPreference='Stop';
$path=$env:KOHDALAB_HUB_PRIVATE_DIRECTORY;
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User;
$acl=New-Object Security.AccessControl.DirectorySecurity;
$acl.SetOwner($sid);
$acl.SetAccessRuleProtection($true,$false);
$rule=New-Object Security.AccessControl.FileSystemAccessRule($sid,'FullControl','ContainerInherit,ObjectInherit','None','Allow');
$acl.AddAccessRule($rule);
# Use .NET Framework ACL methods directly. A Windows PowerShell child can
# inherit PowerShell 7 module paths, which make Security cmdlets unloadable.
[IO.Directory]::SetAccessControl($path,$acl);
$check=[IO.Directory]::GetAccessControl($path);
if(-not $check.AreAccessRulesProtected){throw 'Service directory ACL is not protected'};
foreach($entry in $check.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])) {
 if($entry.AccessControlType -eq 'Allow' -and $entry.IdentityReference.Value -ne $sid.Value){throw 'Unexpected service directory access rule'}
}"#;
        let output = std::process::Command::new(powershell)
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                script,
            ])
            .env("KOHDALAB_HUB_PRIVATE_DIRECTORY", directory)
            .output()
            .map_err(|e| format!("Protect instrument service credentials: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "Cannot restrict instrument service directory to this Windows user: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_directory_refuses_unrelated_data_before_permissions_are_changed() {
        let root = tempfile::tempdir().unwrap();
        let private = root.path().join("hub");
        create_private_directory(&private).unwrap();
        fs::write(private.join("experiment.csv"), "preserve me").unwrap();
        assert!(
            create_private_directory(&private)
                .unwrap_err()
                .contains("unrelated")
        );
        assert_eq!(
            fs::read_to_string(private.join("experiment.csv")).unwrap(),
            "preserve me"
        );
    }
}
