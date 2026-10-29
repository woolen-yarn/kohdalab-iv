//! External control uses the same GUI settings, validation and single engine.
use super::*;
use crate::gui_control::{GuiControl, outputs_off};
use serde_json::{Value, json};

impl IvApp {
    pub(super) fn python_locked(&self) -> bool {
        self.control.as_ref().is_some_and(GuiControl::has_lease)
    }
    fn api_idle(&self, control: &GuiControl) -> Result<(), String> {
        if !self.client.as_ref().is_ok_and(|client| client.is_alive()) {
            return Err("Measurement engine disconnected. Check outputs manually and restart the application.".into());
        }
        if self.busy()
            || self.pending_run
            || control.has_active()
            || self.closing
            || self.timing_open
            || self.save_notice.is_some()
            || self.protection_dialog.is_some()
            || self.start_error.is_some()
        {
            Err("GUI is busy. Finish operations and close dialogs before changing settings.".into())
        } else {
            Ok(())
        }
    }
    pub(super) fn external_control(&mut self) {
        let Some(mut control) = self.control.take() else {
            return;
        };
        if control.expired(self.state.safety_revision) {
            self.send(Command::OutputOff);
            self.report("Python control expired. Stop and output OFF requested.".into());
        }
        if !self.client.as_ref().is_ok_and(|c| c.is_alive()) && control.has_lease() {
            let message = "Measurement engine disconnected. Output OFF cannot be confirmed. Check instrument outputs manually and restart the application.".to_owned();
            control.engine_lost(message.clone());
            self.report(message.clone());
            self.start_error = Some(message);
        }
        if let Some((tag, command)) = control.advance(&self.state) {
            self.dispatch_api_command(&mut control, &tag, command);
        }
        control.settle_release(&self.state);
        for _ in 0..8 {
            let Some(request) = control.next() else {
                break;
            };
            let idle = self.api_idle(&control).is_ok();
            let result = match control.authorize(&request, idle) {
                Ok(Some(value)) => Ok(value),
                Ok(None) => self.external_action(&mut control, &request.method, &request.body),
                Err(error) => Err(error),
            };
            request.respond(result);
        }
        if let Some((tag, command)) = control.advance(&self.state) {
            self.dispatch_api_command(&mut control, &tag, command);
        }
        self.control = Some(control);
    }
    fn dispatch_api_command(&mut self, control: &mut GuiControl, tag: &str, command: Command) {
        let run = matches!(command, Command::Run { .. });
        if run {
            self.protection_dialog = None;
            self.start_error = None;
            self.state.error.clear();
            self.pending_run = true;
            self.pending_base = self.state.run_id;
            self.pending_safety = self.state.safety_revision;
            self.state.phase = Phase::Preparing;
        }
        if let Err(error) = self
            .client
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|c| c.send_job(tag.to_owned(), command))
        {
            if run {
                self.pending_run = false;
            }
            control.fail_dispatch(error.clone());
            self.report(error);
            // A lost dispatch acknowledgement never authorizes another output.
            self.send(Command::OutputOff);
        }
    }
    fn external_action(
        &mut self,
        control: &mut GuiControl,
        method: &str,
        body: &Value,
    ) -> Result<Value, String> {
        match method {
            "capabilities" => {
                return Ok(
                    json!({"application":"kohdalab-iv","application_version":env!("CARGO_PKG_VERSION"),"api_version":2,"actions":["capabilities","status","config","plan","output","acquire","heartbeat","release","detach","configure","load_config","set_plan","set_output","start","result","stop","output_off","connect","disconnect","connect_gate","set_gate_voltage","gate_output_off","disconnect_gate"],"history_limit":32,"request_id":"uuid","fixed_meter_ranges":true,"safety":{"lease_timeout_seconds":10,"on_disconnect":"configurable_default_stop_and_output_off","stop_policy_defaults":{"stop_on_disconnect":true,"stop_on_error":true,"stop_on_timeout":true},"on_release":"verified_output_off","gui_stop_always_available":true}}),
                );
            }
            "status" => {
                return Ok(
                    json!({"snapshot":self.state,"active_run":control.active_id().and_then(|id|control.result(id).ok()),"control":control.control()}),
                );
            }
            "configuration" => {
                return serde_json::to_value(&self.settings.config).map_err(|e| e.to_string());
            }
            "config" => return self.api_config(),
            "plan" => {
                return serde_json::to_value(&self.settings.config).map_err(|e| e.to_string());
            }
            "output" => {
                return Ok(
                    json!({"dir":self.settings.output_dir,"filename":self.settings.filename,"auto_timestamp_suffix":self.settings.auto_suffix}),
                );
            }
            "result" => return control.result(body["run_id"].as_str().ok_or("Missing run_id")?),
            "detach" => {
                if !control.can_detach_without_stop() {
                    return Err("Both exception and disconnect Stop conditions must be disabled before detaching.".into());
                }
                control.detach();
                return Ok(control.control());
            }
            "release" => {
                if !outputs_off(&self.state) || self.busy() || control.has_active() {
                    return Err("Verify Output OFF and wait for cleanup before releasing the control lease.".into());
                }
                if !self.send(Command::OutputOff) {
                    return Err(self.notice.clone());
                }
                control.release(self.state.safety_revision);
                return Ok(control.control());
            }
            "stop" | "output_off" | "gate_output_off" => {
                if let Some(id) = body.get("run_id").and_then(Value::as_str)
                    && control.active_id() != Some(id)
                {
                    return control.result(id);
                }
                control.cancel_pending();
                if !self.send(match method {
                    "stop" => Command::Stop,
                    "gate_output_off" => Command::GateOutputOff,
                    _ => Command::OutputOff,
                }) {
                    return Err(self.notice.clone());
                }
                return if let Some(id) = body.get("run_id").and_then(Value::as_str) {
                    control.result(id)
                } else {
                    Ok(json!({"accepted":true}))
                };
            }
            _ => {}
        }
        let job = matches!(
            method,
            "start"
                | "connect"
                | "disconnect"
                | "connect_gate"
                | "set_gate_voltage"
                | "gate_output_off"
                | "disconnect_gate"
        );
        let id = if job {
            let id = control.job_id(body)?;
            if let Some(existing) = control.existing_job(&id, method, body)? {
                return Ok(existing);
            }
            Some(id)
        } else {
            None
        };
        self.api_idle(control)?;
        if matches!(method, "configure" | "load_config" | "set_plan") {
            let connected = self.state.source_connected
                || self.state.source2_connected
                || self.state.meter_connected
                || self.state.gate_connected;
            if method != "set_plan" && connected {
                return Err("Disconnect instruments before replacing the profile.".into());
            }
            let mut settings = if method == "load_config" {
                load_gui_profile(
                    &std::fs::read(body["path"].as_str().ok_or("Missing config path")?)
                        .map_err(|e| e.to_string())?,
                )?
            } else if method == "configure" {
                if body.get("profile").is_none_or(Value::is_null) {
                    return self.api_config();
                }
                load_gui_profile(
                    &serde_json::to_vec(body.get("profile").ok_or("Missing profile")?)
                        .map_err(|e| e.to_string())?,
                )?
            } else {
                let mut candidate = self.settings.clone();
                candidate.config = serde_json::from_value(body["plan"].clone())
                    .map_err(|e| format!("Invalid plan: {e}"))?;
                if candidate.config.source_model != self.settings.config.source_model
                    || candidate.config.meter_model != self.settings.config.meter_model
                    || candidate
                        .config
                        .balanced_source
                        .as_ref()
                        .zip(self.settings.config.balanced_source.as_ref())
                        .is_some_and(|(a, b)| a.source_model != b.source_model)
                {
                    return Err(
                        "Change instrument models through a disconnected profile, not set_plan."
                            .into(),
                    );
                }
                if self.state.source2_connected
                    && (candidate.config.balanced_source.is_none()
                        || candidate.config.mode == Mode::Vi)
                {
                    return Err("Disconnect Source 2 before leaving Dual mode.".into());
                }
                if candidate.config.balanced_source.is_some()
                    && self.dc_gate_matches(candidate.source2_id)
                {
                    return Err("Source 2 is holding fixed bias. Disconnect it or select another source before Dual V-I.".into());
                }
                candidate.store_active_protection();
                candidate
            };
            // Exact point/range/ratio validation is the same core used by GUI Start.
            if settings.config.balanced_source.is_some() {
                iv_core::balanced_plan(&settings.config)?;
            } else {
                iv_core::plan(&settings.config)?;
            }
            settings.store_active_entries();
            settings.store_active_protection();
            let mode_changed = settings.config.mode != self.settings.config.mode
                || settings.config.balanced_source.is_some()
                    != self.settings.config.balanced_source.is_some();
            self.custom_text = settings
                .config
                .custom_points
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            self.settings = settings;
            self.send_meter_ranges();
            if mode_changed {
                // Mirror the manual GUI mode switch: never relabel an old
                // V-I curve/live reading as a new I-V measurement.
                self.settings.units =
                    [measurement_definition(self.settings.config.mode).preferred_unit_index; 3];
                self.samples.clear();
                self.rebuild_plot_series();
                self.live_samples.clear();
                self.source2_samples.clear();
                self.live_reading = None;
                self.last_live_timestamp.clear();
            }
            self.preview = sweep_preview(&self.settings.config);
            if method == "load_config" {
                self.config_path = body["path"].as_str().unwrap().into();
            }
            if mode_changed && self.state.connected && self.state.source_dc.is_none() {
                return control.begin_job(
                    uuid::Uuid::new_v4().to_string(),
                    "set_plan",
                    vec![Command::SetLiveMode {
                        mode: self.settings.config.mode,
                    }],
                    &self.state,
                    body,
                );
            }
            return self.api_config();
        }
        if method == "set_output" {
            let mut directory = self.settings.output_dir.clone();
            let mut name = self.settings.filename.clone();
            let mut suffix = self.settings.auto_suffix;
            if let Some(v) = body.get("directory").or_else(|| body.get("dir")) {
                directory = v
                    .as_str()
                    .filter(|s| !s.trim().is_empty())
                    .ok_or("directory must be nonempty")?
                    .into();
            }
            if let Some(v) = body.get("filename") {
                name = api_filename(v.as_str().ok_or("filename must be a string")?)?;
            }
            if let Some(v) = body.get("auto_timestamp_suffix") {
                suffix = v.as_bool().ok_or("auto_timestamp_suffix must be boolean")?;
            }
            self.settings.output_dir = directory;
            self.settings.filename = name;
            self.settings.auto_suffix = suffix;
            return Ok(
                json!({"dir":self.settings.output_dir,"filename":self.settings.filename,"auto_timestamp_suffix":self.settings.auto_suffix}),
            );
        }
        let commands = match method {
            "start" => {
                if let Some(reason) = start_block_reason(
                    &self.settings.config,
                    self.state.connected
                        && (self.settings.config.balanced_source.is_none()
                            || self.state.source2_connected),
                    self.state.source_simulation == self.state.meter_simulation
                        && (self.settings.config.balanced_source.is_none()
                            || self.state.source_simulation == self.state.source2_simulation),
                ) {
                    return Err(reason);
                }
                let name = api_filename(
                    api_optional_text(body, "filename")?.unwrap_or(&self.settings.filename),
                )?;
                let directory =
                    api_optional_text(body, "directory")?.unwrap_or(&self.settings.output_dir);
                if directory.trim().is_empty() {
                    return Err("directory must be nonempty".into());
                }
                let basename = if self.settings.auto_suffix {
                    format!(
                        "{}_{}",
                        name,
                        time::OffsetDateTime::now_utc().unix_timestamp_nanos()
                    )
                } else {
                    name
                };
                vec![Command::Run {
                    config: self.settings.config.clone(),
                    path: PathBuf::from(directory)
                        .join(format!("{basename}.csv"))
                        .display()
                        .to_string(),
                }]
            }
            "connect" | "disconnect" => {
                let devices = api_devices(
                    body,
                    self.settings.config.balanced_source.is_some(),
                    method == "disconnect",
                )?;
                let mut commands = Vec::new();
                for device in devices {
                    let command = match (method, device.as_str()) {
                        ("connect", "source") => Command::ConnectSource {
                            model: self.settings.source_model.clone(),
                            resource: self.settings.source.clone(),
                            simulation: self.settings.source_model == "SIMULATED_SOURCE",
                        },
                        ("connect", "source2") => Command::ConnectSource2 {
                            model: self.settings.source2_model.clone(),
                            resource: self.settings.source2.clone(),
                            simulation: self.settings.source2_model == "SIMULATED_SOURCE",
                        },
                        ("connect", "meter") => Command::ConnectMeter {
                            model: self.settings.meter_model.clone(),
                            resource: self.settings.meter.clone(),
                            simulation: self.settings.meter_model == "SIMULATED_METER",
                        },
                        ("disconnect", "source") => Command::DisconnectSource,
                        ("disconnect", "source2") => Command::DisconnectSource2,
                        ("disconnect", "meter") => Command::DisconnectMeter,
                        _ => return Err("Unsupported device role".into()),
                    };
                    commands.push(command);
                }
                commands
            }
            "connect_gate" => vec![Command::ConnectGate {
                resource: api_optional_text(body, "resource")?.unwrap_or("").into(),
                model: api_optional_text(body, "model")?
                    .unwrap_or("YOKOGAWA_GS210")
                    .into(),
                simulation: match body.get("simulation") {
                    None => false,
                    Some(Value::Bool(value)) => *value,
                    _ => return Err("simulation must be boolean".into()),
                },
            }],
            "set_gate_voltage" => vec![Command::SetGateVoltage {
                voltage: api_number(body, "voltage")?,
                compliance: api_number(body, "compliance")?,
                ramp_step: api_number(body, "ramp_step")?,
                range: api_optional_number(body, "range")?,
            }],
            "disconnect_gate" => vec![Command::DisconnectGate],
            _ => return Err("Unknown GUI API method".into()),
        };
        control.begin_job(id.unwrap(), method, commands, &self.state, body)
    }
    fn api_config(&mut self) -> Result<Value, String> {
        self.settings.store_active_entries();
        Ok(json!({"profile":self.settings,"config_path":self.config_path}))
    }
    pub(super) fn python_banner(&mut self, ctx: &egui::Context) {
        if !self.python_locked() {
            return;
        }
        let retiring = self.control.as_ref().is_some_and(GuiControl::is_retiring);
        let control = self.control.as_ref().unwrap().control();
        let mut takeover = false;
        let mut stop = false;
        egui::TopBottomPanel::top("python_control").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(
                    BLUE,
                    if retiring {
                        "PYTHON DISCONNECTED · Verifying output OFF"
                    } else if control["status"] == "detached" {
                        "PYTHON DISCONNECTED · Finishing current operation"
                    } else {
                        "PYTHON CONTROL · GUI controls locked"
                    },
                );
                if let Some(kind) = control["active_run"]["kind"].as_str() {
                    ui.label(kind);
                } else {
                    ui.label("Waiting for Python");
                }
                stop = ui.button("Stop").clicked();
                takeover = ui.button("Take control").clicked();
            });
        });
        if stop {
            if let Some(control) = &mut self.control {
                control.cancel_pending();
            }
            self.send(Command::Stop);
        }
        if takeover {
            if let Some(control) = &mut self.control {
                control.revoke(self.state.safety_revision);
            }
            self.send(Command::OutputOff);
            self.report("Control returned to GUI. Waiting for verified output OFF.".into());
        }
    }
}
fn api_number(body: &Value, key: &str) -> Result<f64, String> {
    body[key]
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("{key} must be a finite number"))
}
fn api_optional_number(body: &Value, key: &str) -> Result<Option<f64>, String> {
    match body.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(value)) => value
            .as_f64()
            .filter(|value| value.is_finite())
            .map(Some)
            .ok_or_else(|| format!("{key} must be a finite number")),
        _ => Err(format!("{key} must be a finite number")),
    }
}
fn api_filename(name: &str) -> Result<String, String> {
    let name = name.trim();
    let stem = if name.to_ascii_lowercase().ends_with(".csv") {
        &name[..name.len() - 4]
    } else {
        name
    };
    if stem.is_empty() || stem.contains(['/', '\\']) || stem == "." || stem == ".." {
        return Err("filename must be a basename; use directory for the path".into());
    }
    Ok(stem.into())
}
fn api_devices(body: &Value, dual: bool, disconnect: bool) -> Result<Vec<String>, String> {
    let devices = if let Some(value) = body.get("devices").filter(|v| !v.is_null()) {
        serde_json::from_value::<Vec<String>>(value.clone())
            .map_err(|_| "devices must be a list of roles")?
    } else {
        let mut v = vec!["source".into()];
        if dual || disconnect {
            v.push("source2".into());
        }
        v.push("meter".into());
        v
    };
    let mut seen = std::collections::HashSet::new();
    if devices.is_empty()
        || devices.iter().any(|v| {
            !matches!(v.as_str(), "source" | "source2" | "meter") || !seen.insert(v.clone())
        })
    {
        return Err("devices must contain unique source, source2 or meter roles".into());
    }
    Ok(devices)
}

fn api_optional_text<'a>(body: &'a Value, key: &str) -> Result<Option<&'a str>, String> {
    match body.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        _ => Err(format!("{key} must be a string")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn output_names_and_device_lists_are_validated_before_mutation() {
        assert_eq!(api_filename("  sample.CSV ").unwrap(), "sample");
        for name in ["", ".csv", "../file", "folder\\file", ".", ".."] {
            assert!(api_filename(name).is_err());
        }
        assert_eq!(
            api_devices(&json!({}), true, false).unwrap(),
            vec!["source", "source2", "meter"]
        );
        for devices in [
            json!([]),
            json!(["source", "source"]),
            json!(["gate"]),
            json!("source"),
        ] {
            assert!(api_devices(&json!({"devices":devices}), false, false).is_err());
        }
    }
}
