//! Safe export of an already-completed measurement run.
//!
//! The CSV is copied as bytes, rather than being parsed and rewritten, so an
//! export cannot change numeric formatting or discard fields that are not
//! currently displayed by the application.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::Path;

/// Copies a completed run's CSV and its `.run.json` / `.raw.csv` companions to a new path.
///
/// Existing destination files are never overwritten.  The source CSV and its
/// metadata are only read.  If writing either destination file fails, this
/// function removes only files created by this call.
pub fn save_run_copy(source: &Path, destination: &Path) -> Result<(), String> {
    if same_location(source, destination)? {
        return Err("Choose a different filename when saving a copy of this run.".into());
    }

    let source_metadata = source.with_extension("run.json");
    let destination_metadata = destination.with_extension("run.json");
    if !destination
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("csv"))
    {
        return Err("Choose a .csv filename for the measurement; its settings are saved separately as .run.json.".into());
    }

    let metadata_bytes = fs::read(&source_metadata).map_err(|error| {
        format!(
            "Cannot save a copy because the run metadata file is missing or unreadable ({}): {error}",
            source_metadata.display()
        )
    })?;
    let mut metadata = parse_completed_metadata(&metadata_bytes, &source_metadata)?;
    // Historical runs keep their JSONL sidecar; new runs use the tabular raw
    // evidence sidecar. Save never rewrites either format.
    let audit_key = if metadata.get("raw_csv").is_some() {
        "raw_csv"
    } else {
        "readings_audit"
    };
    let source_audit = if audit_key == "raw_csv" {
        source.with_extension("raw.csv")
    } else {
        source.with_extension("readings.jsonl")
    };
    let destination_audit = if audit_key == "raw_csv" {
        destination.with_extension("raw.csv")
    } else {
        destination.with_extension("readings.jsonl")
    };
    let audit_required = metadata.get(audit_key).is_some();

    if destination.exists() {
        return Err(format!(
            "Cannot save a copy because the destination CSV already exists: {}. Choose a new filename.",
            destination.display()
        ));
    }
    if destination_metadata.exists() {
        return Err(format!(
            "Cannot save a copy because the destination metadata file already exists: {}. Choose a new filename.",
            destination_metadata.display()
        ));
    }
    if audit_required && destination_audit.exists() {
        return Err(format!(
            "Cannot save a copy because the destination readings audit already exists: {}. Choose a new filename.",
            destination_audit.display()
        ));
    }

    let source_file = File::open(source).map_err(|error| {
        format!(
            "Cannot save a copy because the source CSV cannot be read ({}): {error}",
            source.display()
        )
    })?;
    let source_audit_file = if audit_required {
        Some(File::open(&source_audit).map_err(|error| format!(
            "Cannot save a copy because the readings audit is missing or unreadable ({}): {error}",
            source_audit.display()
        ))?)
    } else {
        None
    };

    let source_display = source.display().to_string();
    let destination_display = destination.display().to_string();
    let outcome = metadata
        .get_mut("outcome")
        .and_then(serde_json::Value::as_object_mut)
        .expect("metadata was validated before mutation");
    outcome.insert(
        "path".into(),
        serde_json::Value::String(destination_display.clone()),
    );
    metadata["csv"] = serde_json::Value::String(destination_display);
    metadata["exported_from"] = serde_json::Value::String(source_display);
    if audit_required {
        metadata[audit_key]["path"] =
            serde_json::Value::String(destination_audit.display().to_string());
        metadata[audit_key]["filename"] = serde_json::Value::String(
            destination_audit
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or(if audit_key == "raw_csv" {
                    "raw.csv"
                } else {
                    "readings.jsonl"
                })
                .to_owned(),
        );
    }

    let mut created_csv = false;
    let mut created_metadata = false;
    let mut created_audit = false;
    let result = (|| -> Result<(), String> {
        let mut destination_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .map_err(|error| {
                format!(
                    "Cannot create the destination CSV ({}): {error}. Choose a writable new filename.",
                    destination.display()
                )
            })?;
        created_csv = true;
        let mut source_file = source_file;
        io::copy(&mut source_file, &mut destination_file).map_err(|error| {
            format!(
                "Could not copy the complete CSV to {}: {error}",
                destination.display()
            )
        })?;
        destination_file.sync_all().map_err(|error| {
            format!(
                "Could not finish saving the CSV copy ({}): {error}",
                destination.display()
            )
        })?;

        if let Some(mut source_audit_file) = source_audit_file {
            let mut destination_audit_file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&destination_audit)
                .map_err(|error| {
                    format!(
                        "Cannot create the destination readings audit ({}): {error}.",
                        destination_audit.display()
                    )
                })?;
            created_audit = true;
            io::copy(&mut source_audit_file, &mut destination_audit_file).map_err(|error| {
                format!(
                    "Could not copy the complete readings audit to {}: {error}",
                    destination_audit.display()
                )
            })?;
            destination_audit_file.sync_all().map_err(|error| {
                format!(
                    "Could not finish saving the readings audit copy ({}): {error}",
                    destination_audit.display()
                )
            })?;
        }

        if metadata.get("file_hashes").is_some() {
            metadata["file_hashes"] = serde_json::json!({
                "csv_sha256": crate::raw_csv::file_sha256(destination)?,
                "raw_csv_sha256": if audit_key == "raw_csv" {
                    Some(crate::raw_csv::file_sha256(&destination_audit)?)
                } else { None },
            });
        }

        let mut metadata_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination_metadata)
            .map_err(|error| {
                format!(
                    "Cannot create the destination metadata file ({}): {error}. A partial CSV copy may have been created.",
                    destination_metadata.display()
                )
            })?;
        created_metadata = true;
        serde_json::to_writer_pretty(&mut metadata_file, &metadata).map_err(|error| {
            format!(
                "Could not write the destination metadata ({}): {error}",
                destination_metadata.display()
            )
        })?;
        metadata_file.sync_all().map_err(|error| {
            format!(
                "Could not finish saving the metadata copy ({}): {error}",
                destination_metadata.display()
            )
        })?;
        Ok(())
    })();

    if let Err(error) = result {
        let mut cleanup_failures = Vec::new();
        if created_metadata && let Err(cleanup_error) = fs::remove_file(&destination_metadata) {
            cleanup_failures.push(format!(
                "{} ({cleanup_error})",
                destination_metadata.display()
            ));
        }
        if created_audit && let Err(cleanup_error) = fs::remove_file(&destination_audit) {
            cleanup_failures.push(format!("{} ({cleanup_error})", destination_audit.display()));
        }
        if created_csv && let Err(cleanup_error) = fs::remove_file(destination) {
            cleanup_failures.push(format!("{} ({cleanup_error})", destination.display()));
        }
        if !cleanup_failures.is_empty() {
            return Err(format!(
                "{error} Partial export files could not be removed: {}. Remove only these partial files before trying again.",
                cleanup_failures.join(", ")
            ));
        }
        return Err(error);
    }

    Ok(())
}

fn parse_completed_metadata(
    bytes: &[u8],
    metadata_path: &Path,
) -> Result<serde_json::Value, String> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|error| {
        format!(
            "Cannot save a copy because the run metadata is invalid JSON ({}): {error}",
            metadata_path.display()
        )
    })?;
    let object = value.as_object().ok_or_else(|| {
        format!(
            "Cannot save a copy because the run metadata is invalid ({}): expected an object.",
            metadata_path.display()
        )
    })?;
    let completed = object
        .get("in_progress")
        .and_then(serde_json::Value::as_bool)
        .map(|in_progress| !in_progress)
        .unwrap_or(false);
    let valid = completed
        && object
            .get("csv")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !value.is_empty())
        && object
            .get("config")
            .is_some_and(serde_json::Value::is_object)
        && object
            .get("requested_config")
            .is_some_and(serde_json::Value::is_object)
        && object
            .get("outcome")
            .and_then(serde_json::Value::as_object)
            .and_then(|outcome| outcome.get("path"))
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !value.is_empty());
    if !valid {
        return Err(format!(
            "Cannot save a copy because the run metadata is incomplete or the run is still in progress ({}). Wait for the run to finish, then try again.",
            metadata_path.display()
        ));
    }
    Ok(value)
}

fn same_location(source: &Path, destination: &Path) -> Result<bool, String> {
    if source == destination {
        return Ok(true);
    }
    let source_canonical = fs::canonicalize(source).map_err(|error| {
        format!(
            "Cannot save a copy because the source CSV cannot be resolved ({}): {error}",
            source.display()
        )
    })?;
    let destination_location = if destination.exists() {
        fs::canonicalize(destination).map_err(|error| {
            format!(
                "Cannot resolve the destination CSV ({}): {error}",
                destination.display()
            )
        })?
    } else {
        let parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let parent = fs::canonicalize(parent).map_err(|error| {
            format!(
                "Cannot resolve the destination folder ({}): {error}",
                parent.display()
            )
        })?;
        parent.join(destination.file_name().ok_or_else(|| {
            format!(
                "Choose a filename for the destination CSV: {}",
                destination.display()
            )
        })?)
    };
    Ok(source_canonical == destination_location)
}

#[cfg(test)]
mod tests {
    use super::save_run_copy;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("kohdalab-iv-run-export-{name}-{stamp}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_run(csv: &Path) -> Vec<u8> {
        let bytes = b"voltage_V,current_A\n1.2345678901234567e-6,-2.0e-9\n".to_vec();
        fs::write(csv, &bytes).unwrap();
        let audit = b"{\"event\":\"run_started\",\"raw_response\":\"1.23000e-6\"}\n";
        fs::write(csv.with_extension("readings.jsonl"), audit).unwrap();
        let metadata = serde_json::json!({
            "schema_version": 5,
            "in_progress": false,
            "csv": csv.display().to_string(),
            "config": {"mode": "Vi"},
            "requested_config": {"mode": "Vi", "step": 1e-6},
            "outcome": {"path": csv.display().to_string(), "phase": "Completed"},
            "readings_audit": {"path": csv.with_extension("readings.jsonl").display().to_string(), "schema": "iv_readings_v2"},
            "points": [{"voltage_V": 1.2345678901234567e-6}]
        });
        fs::write(
            csv.with_extension("run.json"),
            serde_json::to_vec_pretty(&metadata).unwrap(),
        )
        .unwrap();
        bytes
    }

    #[test]
    fn rejects_non_csv_destinations_before_creating_any_files() {
        let directory = temp_dir("sidecar-collision");
        let source = directory.join("source.csv");
        let destination = directory.join("copy.run.json");
        let original = write_run(&source);
        assert!(
            save_run_copy(&source, &destination)
                .unwrap_err()
                .contains("Choose a .csv filename")
        );
        assert!(!destination.exists());
        assert_eq!(fs::read(&source).unwrap(), original);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn exports_exact_csv_bytes_and_updates_only_export_locations() {
        let directory = temp_dir("exact");
        let source = directory.join("source.csv");
        let destination = directory.join("saved-copy.csv");
        let bytes = write_run(&source);

        save_run_copy(&source, &destination).unwrap();

        assert_eq!(fs::read(&destination).unwrap(), bytes);
        let metadata: serde_json::Value =
            serde_json::from_slice(&fs::read(destination.with_extension("run.json")).unwrap())
                .unwrap();
        assert_eq!(metadata["csv"], destination.display().to_string());
        assert_eq!(
            metadata["outcome"]["path"],
            destination.display().to_string()
        );
        assert_eq!(metadata["exported_from"], source.display().to_string());
        assert_eq!(metadata["requested_config"]["step"], 1e-6);
        assert_eq!(
            fs::read(destination.with_extension("readings.jsonl")).unwrap(),
            fs::read(source.with_extension("readings.jsonl")).unwrap()
        );
        assert_eq!(
            metadata["readings_audit"]["path"],
            destination
                .with_extension("readings.jsonl")
                .display()
                .to_string()
        );
        assert_eq!(fs::read(&source).unwrap(), bytes);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn preserves_preexisting_destination_csv_and_sidecar() {
        let directory = temp_dir("existing");
        let source = directory.join("source.csv");
        let destination = directory.join("saved-copy.csv");
        write_run(&source);

        fs::write(&destination, b"existing csv").unwrap();
        let error = save_run_copy(&source, &destination).unwrap_err();
        assert!(error.contains("destination CSV already exists"));
        assert_eq!(fs::read(&destination).unwrap(), b"existing csv");

        fs::remove_file(&destination).unwrap();
        fs::write(destination.with_extension("run.json"), b"existing metadata").unwrap();
        let error = save_run_copy(&source, &destination).unwrap_err();
        assert!(error.contains("destination metadata file already exists"));
        assert_eq!(
            fs::read(destination.with_extension("run.json")).unwrap(),
            b"existing metadata"
        );
        assert!(!destination.exists());

        fs::remove_file(destination.with_extension("run.json")).unwrap();
        fs::write(
            destination.with_extension("readings.jsonl"),
            b"existing audit",
        )
        .unwrap();
        let error = save_run_copy(&source, &destination).unwrap_err();
        assert!(error.contains("destination readings audit already exists"));
        assert_eq!(
            fs::read(destination.with_extension("readings.jsonl")).unwrap(),
            b"existing audit"
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn dual_source_details_stay_in_json_when_saving_a_copy() {
        let directory = temp_dir("dual-details");
        let source = directory.join("source.csv");
        let csv = b"voltage_V,current_A\n1e-3,1e-6\n";
        fs::write(&source, csv).unwrap();
        fs::write(
            source.with_extension("raw.csv"),
            b"run_id,event_type\nrun,reading\n",
        )
        .unwrap();
        let points = serde_json::json!([{"point_index":1,
            "source1":{"setpoint_V":0.0005,"level_readback_V":0.0005,"output_voltage_V":null},
            "source2":{"setpoint_V":-0.0005,"level_readback_V":-0.0005,"output_voltage_V":null}}]);
        let metadata = serde_json::json!({"schema_version":crate::compatibility::RUN_METADATA_SCHEMA_VERSION,
            "in_progress":false,"csv":source.display().to_string(),"config":{},"requested_config":{},
            "outcome":{"path":source.display().to_string(),"phase":"Completed"},
            "balanced_drive":{"ratio":[1,1]},"dual_source_points":points,
            "raw_csv":{"path":source.with_extension("raw.csv").display().to_string(),"schema":"raw_csv_v1"}});
        fs::write(
            source.with_extension("run.json"),
            serde_json::to_vec(&metadata).unwrap(),
        )
        .unwrap();
        let destination = directory.join("copy.csv");
        save_run_copy(&source, &destination).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), csv);
        let copied: serde_json::Value =
            serde_json::from_slice(&fs::read(destination.with_extension("run.json")).unwrap())
                .unwrap();
        assert_eq!(copied["balanced_drive"]["ratio"], serde_json::json!([1, 1]));
        assert_eq!(copied["dual_source_points"], points);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn raw_csv_export_recomputes_destination_hashes() {
        let directory = temp_dir("hashes");
        let source = directory.join("source.csv");
        fs::write(&source, b"run_id,point_index\nrun,1\n").unwrap();
        fs::write(
            source.with_extension("raw.csv"),
            b"run_id,event_type\nrun,reading\n",
        )
        .unwrap();
        let metadata = serde_json::json!({
            "schema_version": 6, "in_progress": false,
            "csv": source.display().to_string(),
            "config": {}, "requested_config": {},
            "outcome": {"path": source.display().to_string(), "phase": "Completed"},
            "raw_csv": {"path": source.with_extension("raw.csv").display().to_string(), "schema": "raw_csv_v1"},
            "file_hashes": {"csv_sha256": "old", "raw_csv_sha256": "old"}
        });
        fs::write(
            source.with_extension("run.json"),
            serde_json::to_vec(&metadata).unwrap(),
        )
        .unwrap();
        let destination = directory.join("copy.csv");
        save_run_copy(&source, &destination).unwrap();
        let copied: serde_json::Value =
            serde_json::from_slice(&fs::read(destination.with_extension("run.json")).unwrap())
                .unwrap();
        assert_eq!(
            copied["file_hashes"]["csv_sha256"],
            crate::raw_csv::file_sha256(&destination).unwrap()
        );
        assert_eq!(
            copied["file_hashes"]["raw_csv_sha256"],
            crate::raw_csv::file_sha256(&destination.with_extension("raw.csv")).unwrap()
        );
        assert_eq!(copied["raw_csv"]["filename"], "copy.raw.csv");
        assert!(copied.get("readings_audit").is_none());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_same_path_and_missing_or_invalid_metadata() {
        let directory = temp_dir("invalid");
        let source = directory.join("source.csv");
        write_run(&source);

        let error = save_run_copy(&source, &source).unwrap_err();
        assert!(error.contains("different filename"));

        fs::remove_file(source.with_extension("run.json")).unwrap();
        let error = save_run_copy(&source, &directory.join("copy.csv")).unwrap_err();
        assert!(error.contains("metadata file is missing"));

        fs::write(source.with_extension("run.json"), b"{not json").unwrap();
        let error = save_run_copy(&source, &directory.join("copy.csv")).unwrap_err();
        assert!(error.contains("invalid JSON"));

        fs::write(
            source.with_extension("run.json"),
            br#"{"in_progress":false,"csv":"source.csv"}"#,
        )
        .unwrap();
        let error = save_run_copy(&source, &directory.join("copy.csv")).unwrap_err();
        assert!(error.contains("incomplete"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn accepts_a_plain_relative_destination_path_for_location_checks() {
        let directory = temp_dir("relative");
        let source = directory.join("source.csv");
        write_run(&source);

        assert!(!super::same_location(&source, Path::new("copy.csv")).unwrap());
        fs::remove_dir_all(directory).unwrap();
    }
}
