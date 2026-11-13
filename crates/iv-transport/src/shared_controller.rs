//! Hardware side of the shared service. Never selected by an application.
//! Holds the legacy whole-adapter lock and never resets/reopens after a fault.
use super::*;

pub struct NativeController {
    session: Arc<Mutex<HelperSession>>,
}

impl NativeController {
    pub fn open(helper: &Path) -> Result<Self, String> {
        let lease = DeviceLease::acquire_physical("gpib-adapter")?;
        let session = HelperSession::spawn_physical(helper, STARTUP_TIMEOUT, lease)?;
        Ok(Self {
            session: Arc::new(Mutex::new(session)),
        })
    }

    pub fn transfer(
        &mut self,
        operation: &str,
        address: u8,
        timeout: Duration,
        payload: &[u8],
    ) -> Result<String, String> {
        if !(1..=30).contains(&address)
            || !matches!(
                operation,
                "WRITE" | "QUERY" | "READ" | "CLEAR" | "LOCAL" | "LIST"
            )
        {
            return Err("Invalid controller operation or address".into());
        }
        let mut session = lock_session(&self.session)?;
        if session.poisoned {
            return Err("Shared GPIB controller faulted; no bus reset or write replay was attempted. Verify outputs at the front panel.".into());
        }
        session.request(
            operation,
            address,
            timeout.as_millis().clamp(1, 30_000) as u64,
            payload,
            timeout + Duration::from_millis(100),
        )
    }

    pub fn valid_source(model: &str) -> bool {
        instruments::valid_model(Role::Source, model)
    }

    /// Identity must be verified before an orphaned source can be recovered.
    pub fn verify_source(&mut self, address: u8, model: &str) -> Result<(), String> {
        if !Self::valid_source(model) {
            return Err("Unsupported source cleanup model".into());
        }
        let endpoint = self.endpoint(address);
        let identity = identify_endpoint_before(
            &endpoint,
            Role::Source,
            model,
            Instant::now() + Duration::from_secs(5),
        )?;
        instruments::verify_model(Role::Source, model, &identity)
    }

    /// Reuses the IV source dialect and OFF verification, without its
    /// single-application helper replacement/recovery (which would reset peers).
    pub fn source_off(&mut self, address: u8, model: &str) -> Result<(), String> {
        if !(1..=30).contains(&address) || !Self::valid_source(model) {
            return Err("Unsupported source cleanup address or model".into());
        }
        {
            let mut session = lock_session(&self.session)?;
            if session.poisoned {
                // The helper pipe may still deliver the addressed OFF write.
                // Its replies cannot be correlated after a failed transaction.
                session.send_request(
                    "WRITE",
                    address,
                    SCPI_TIMEOUT_MS,
                    source_off_command(model).as_bytes(),
                )?;
                return Err("Attempted source OFF after the controller fault. Output OFF could not be verified on the faulted session.".into());
            }
        }
        let endpoint = self.endpoint(address);
        let mut pair = DevicePair::new(&HelperPaths {
            gpib: PathBuf::new(),
            usbtmc: PathBuf::new(),
            setup: None,
        });
        pair.source_model = model.to_owned();
        pair.source = Some(endpoint);
        let result = pair.output_off_once(Role::Source, Instant::now() + Duration::from_secs(5));
        // Do not let DevicePair::Drop perform whole-session REN/EXIT teardown.
        pair.source = None;
        result.map(|_| ())
    }

    fn endpoint(&self, address: u8) -> Endpoint {
        Endpoint {
            resource: format!("GPIB0::{address}::INSTR"),
            address,
            session: self.session.clone(),
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn faulted_controller_dispatches_addressed_off_without_trusting_a_reply() {
        let name = format!("iv-faulted-shared-helper-{}", std::process::id());
        let dir = std::env::temp_dir().join(&name);
        fs::create_dir_all(&dir).unwrap();
        let log = dir.join("off.log");
        let helper = dir.join("helper");
        fs::write(
            &helper,
            format!(
                r#"#!/bin/sh
printf 'KIV\tOK\t\n'
while read op address timeout payload; do
  case "$op" in
    QUERY) printf 'KIV\tERR\t6661756c74\n' ;;
    WRITE) printf '%s %s\n' "$address" "$payload" >> '{}'
           printf 'KIV\tOK\t30\n' ;;
    EXIT)  printf 'KIV\tOK\t\n'; exit ;;
  esac
done
"#,
                log.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        let session = HelperSession::spawn_physical(
            &helper,
            STARTUP_TIMEOUT,
            DeviceLease::acquire(&name).unwrap(),
        )
        .unwrap();
        let mut controller = NativeController {
            session: Arc::new(Mutex::new(session)),
        };
        assert!(
            controller
                .transfer("QUERY", 7, Duration::from_secs(1), b"READ?\n")
                .is_err()
        );
        assert!(controller.source_off(3, "YOKOGAWA_GS210").is_err());
        let deadline = Instant::now() + Duration::from_secs(2);
        while !log.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let recorded = fs::read_to_string(&log);
        drop(controller);
        fs::remove_dir_all(dir).unwrap();
        assert_eq!(recorded.unwrap(), "3 3a4f555450204f46460a\n");
    }
}
