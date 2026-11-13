//! Typed internal state with the established JSON strings at the client boundary.
use serde::{Deserialize, Serialize};
use std::{fmt, ops::Deref};

macro_rules! wire_states {
    ($(#[$enum_meta:meta])* $name:ident { $($(#[$variant_meta:meta])* $variant:ident => $label:literal),+ $(,)? }) => {
        $(#[$enum_meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
        pub(crate) enum $name { $($(#[$variant_meta])* #[serde(rename = $label)] $variant),+ }
        impl $name {
            pub(crate) const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $label),+ }
            }
            pub(crate) fn to_lowercase(self) -> String { self.as_str().to_lowercase() }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(self.as_str()) }
        }
        impl PartialEq<&str> for $name {
            fn eq(&self, other: &&str) -> bool { self.as_str() == *other }
        }
    };
}
wire_states!(Phase {
    Idle => "Idle", Scanning => "Scanning", Connecting => "Connecting",
    Ready => "Ready", SwitchingMode => "Switching mode", Preparing => "Preparing",
    Running => "Running", CleaningUp => "Cleaning up", Failed => "Failed",
    ComplianceStop => "Compliance stop", SourceLimitStop => "Source limit stop", Stopped => "Stopped", Completed => "Completed",
});
wire_states!(#[derive(Default)] OutputState { #[default] Unknown => "Unknown", Off => "Off", On => "On" });

/// Internal execution activity, independent of the unchanged UI phase labels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Activity {
    #[default]
    Idle,
    Preflight,
    Configuring,
    Ramping,
    Settling,
    Acquiring,
    Saving,
    ReturningToZero,
    VerifyingOutputOff,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ErrorKind {
    #[default]
    None,
    InvalidConfiguration,
    IdentityMismatch,
    Timeout,
    Disconnected,
    Overload,
    Storage,
    OutputSafety,
    Protocol,
    Other,
}

/// Keep `error` as the same string on the wire; add a machine-readable category.
/// Classification of legacy String errors is confined to this adapter. Categories
/// are diagnostic only and must never decide whether output shutdown is required.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Failure {
    #[serde(rename = "error")]
    message: String,
    #[serde(default, rename = "error_kind")]
    pub(crate) kind: ErrorKind,
}
impl Failure {
    pub(crate) fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind,
        }
    }
    pub(crate) fn as_str(&self) -> &str {
        &self.message
    }
    pub(crate) fn clear(&mut self) {
        self.message.clear();
        self.kind = ErrorKind::None;
    }
    pub(crate) fn push_str(&mut self, value: &str) {
        self.message.push_str(value);
    }
}
impl From<String> for Failure {
    fn from(message: String) -> Self {
        let text = message.to_ascii_lowercase();
        let kind = if text.is_empty() {
            ErrorKind::None
        } else if text.contains("output off") || text.contains("source output") {
            ErrorKind::OutputSafety
        } else if text.contains("identity") || text.contains("matching models") {
            ErrorKind::IdentityMismatch
        } else if text.contains("timeout") || text.contains("timed out") {
            ErrorKind::Timeout
        } else if text.contains("overload") {
            ErrorKind::Overload
        } else if text.contains("csv")
            || text.contains("result folder")
            || text.contains("metadata")
        {
            ErrorKind::Storage
        } else if text.contains("disconnected")
            || text.contains("reconnect")
            || text.contains("transport")
            || text.contains("engine exited")
        {
            ErrorKind::Disconnected
        } else if text.contains("protocol") || text.contains("invalid response") {
            ErrorKind::Protocol
        } else if text.contains("invalid")
            || text.contains("limit")
            || text.contains("range")
            || text.contains("must")
        {
            ErrorKind::InvalidConfiguration
        } else {
            ErrorKind::Other
        };
        Self::new(kind, message)
    }
}
impl From<&str> for Failure {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}
impl Deref for Failure {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_states_keep_legacy_wire_labels() {
        for phase in [
            Phase::Idle,
            Phase::Scanning,
            Phase::Connecting,
            Phase::Ready,
            Phase::SwitchingMode,
            Phase::Preparing,
            Phase::Running,
            Phase::CleaningUp,
            Phase::Failed,
            Phase::ComplianceStop,
            Phase::SourceLimitStop,
            Phase::Stopped,
            Phase::Completed,
        ] {
            let json = serde_json::to_string(&phase).unwrap();
            assert_eq!(json, format!("\"{phase}\""));
            assert_eq!(serde_json::from_str::<Phase>(&json).unwrap(), phase);
        }
        for output in [OutputState::Unknown, OutputState::Off, OutputState::On] {
            assert_eq!(
                serde_json::to_string(&output).unwrap(),
                format!("\"{output}\"")
            );
        }
    }
    #[test]
    fn error_categories_preserve_original_messages() {
        for (message, kind) in [
            ("USB query timed out", ErrorKind::Timeout),
            ("Meter overload", ErrorKind::Overload),
            ("Cannot save CSV", ErrorKind::Storage),
            ("Output off was not verified", ErrorKind::OutputSafety),
        ] {
            let mut failure = Failure::from(message);
            assert_eq!(failure.kind, kind);
            assert_eq!(failure.as_str(), message);
            let json = serde_json::to_value(&failure).unwrap();
            assert_eq!(json["error"], message);
            failure.clear();
            assert!(failure.is_empty());
            assert_eq!(failure.kind, ErrorKind::None);
        }
    }
}
