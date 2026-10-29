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
        if !Self::valid_source(model) {
            return Err("Unsupported source cleanup model".into());
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
