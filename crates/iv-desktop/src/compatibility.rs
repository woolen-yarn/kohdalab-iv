//! Versioned contracts shared by the desktop engine, saved settings and clients.
//!
//! Keep these values deliberately small and explicit.  A new version is a
//! compatibility decision, not merely an application release number.

use serde::{Deserialize, Serialize};

/// On-disk GUI settings understood by this application.
pub(crate) const SETTINGS_SCHEMA_VERSION: u32 = 3;
/// Sidecar `.run.json` metadata understood by this application.
pub(crate) const RUN_METADATA_SCHEMA_VERSION: u32 = 8;
/// CSV field contract written by the native engine.
pub(crate) const CSV_SCHEMA: &str = "analysis_v3";
/// JSON-lines engine protocol understood by bundled clients.
pub(crate) const API_VERSION: u32 = 1;

/// Compatibility declaration included in each engine state snapshot.
///
/// Missing declarations are intentionally treated as the original v1 protocol
/// by clients, allowing installations released before this declaration to keep
/// working.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct ProtocolInfo {
    pub(crate) version: u32,
    pub(crate) minimum_client_version: u32,
    pub(crate) settings_schema_version: u32,
    pub(crate) run_metadata_schema_version: u32,
    pub(crate) csv_schema: String,
}

impl Default for ProtocolInfo {
    fn default() -> Self {
        Self {
            version: API_VERSION,
            minimum_client_version: API_VERSION,
            settings_schema_version: SETTINGS_SCHEMA_VERSION,
            run_metadata_schema_version: RUN_METADATA_SCHEMA_VERSION,
            csv_schema: CSV_SCHEMA.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_protocol_describes_current_contracts() {
        assert_eq!(
            ProtocolInfo::default(),
            ProtocolInfo {
                version: 1,
                minimum_client_version: 1,
                settings_schema_version: 3,
                run_metadata_schema_version: 8,
                csv_schema: "analysis_v3".into(),
            }
        );
    }
}
