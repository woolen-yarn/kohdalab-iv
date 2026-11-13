//! Compact, human-readable lifecycle log for a measurement output bundle.
//!
//! This is deliberately not a transport trace: raw commands and replies belong
//! only in the raw CSV evidence stream.
use serde_json::Value;
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
};

pub struct RunLog {
    file: File,
    run_id: String,
}

impl RunLog {
    pub fn create(path: &Path, run_id: &str) -> Result<Self, String> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            file,
            run_id: run_id.into(),
        })
    }

    /// Append a durable lifecycle/progress/error entry. Context is intentionally
    /// small and raw transport payloads are removed defensively.
    pub fn append(&mut self, level: &str, event: &str, mut context: Value) -> Result<(), String> {
        if let Some(object) = context.as_object_mut() {
            for key in [
                "raw_response",
                "wire_response",
                "command",
                "wire_command",
                "exchanges",
            ] {
                object.remove(key);
            }
        }
        writeln!(
            self.file,
            "{}\t{}\t{}\t{}\t{}",
            timestamp(),
            level,
            self.run_id,
            event,
            serde_json::to_string(&context).map_err(|error| error.to_string())?
        )
        .map_err(|error| error.to_string())?;
        self.file.flush().map_err(|error| error.to_string())
    }

    pub fn finish(&mut self) -> Result<(), String> {
        self.file.flush().map_err(|error| error.to_string())?;
        self.file.sync_all().map_err(|error| error.to_string())
    }
}

fn timestamp() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:06}Z",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
        now.nanosecond() / 1_000
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn log_is_create_new_and_excludes_transport_payloads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run.log");
        let mut log = RunLog::create(&path, "run-test").unwrap();
        log.append(
            "ERROR",
            "run_finished",
            serde_json::json!({"rows": 1,
            "raw_response": "secret", "command": "READ?"}),
        )
        .unwrap();
        log.finish().unwrap();
        let line = std::fs::read_to_string(&path).unwrap();
        assert!(line.contains("\tERROR\trun-test\trun_finished\t"));
        assert!(line.contains("\"rows\":1"));
        assert!(!line.contains("secret") && !line.contains("READ?"));
        assert!(RunLog::create(&path, "other").is_err());
    }
}
