//! Map authenticated external requests to the same GUI actions and device owner.
use super::*;

impl IvApp {
    pub(super) fn external_control(&mut self) {
        let Some(mut control) = self.control.take() else {
            return;
        };
        if control.expired() {
            self.send(Command::OutputOff);
            self.report("Python control connection expired. Stop and output OFF requested.".into());
        }
        // Bound work per frame so local callers cannot starve GUI Stop/rendering.
        for _ in 0..8 {
            let Some(request) = control.next() else {
                break;
            };
            let result = match control.authorize(&request) {
                Ok(Some(value)) => Ok(value),
                Ok(None) => self.external_action(&request.action),
                Err(error) => Err(error),
            };
            if matches!(request.action, crate::gui_control::Action::Release) && result.is_ok() {
                control.release();
            }
            request.respond(result);
        }
        self.control = Some(control);
    }

    fn external_action(
        &mut self,
        action: &crate::gui_control::Action,
    ) -> Result<serde_json::Value, String> {
        use crate::gui_control::Action;
        let command = match action {
            Action::Status => return serde_json::to_value(&self.state).map_err(|e| e.to_string()),
            Action::Configuration => {
                return serde_json::to_value(&self.settings.config).map_err(|e| e.to_string());
            }
            Action::Start {
                filename,
                directory,
            } => {
                if self.busy() || self.pending_run || self.timing_open || self.closing {
                    return Err("GUI is busy. Finish the current operation or close Settings before starting.".into());
                }
                // External parameters belong in the requested output name; no
                // measurement settings or new metadata fields are introduced.
                let previous = (
                    self.settings.filename.clone(),
                    self.settings.output_dir.clone(),
                );
                if let Some(name) = filename {
                    self.settings.filename = name.clone();
                }
                if let Some(path) = directory {
                    self.settings.output_dir = path.clone();
                }
                let base = self.state.run_id;
                self.start_run_impl(false);
                self.settings.filename = previous.0;
                self.settings.output_dir = previous.1;
                if !self.pending_run {
                    return Err(self.start_error.clone().unwrap_or_else(|| {
                        "GUI could not start measurement. Check its log.".into()
                    }));
                }
                return Ok(
                    serde_json::json!({"run_id":base+1,"safety_revision":self.pending_safety}),
                );
            }
            Action::Stop => Command::Stop,
            Action::OutputOff | Action::Release => Command::OutputOff,
            Action::GateOutputOff => Command::GateOutputOff,
            Action::ConnectGate {
                resource,
                simulation,
                model,
            } => {
                if self.busy() || self.pending_run {
                    return Err("Wait for measurement to finish before connecting Gate.".into());
                }
                Command::ConnectGate {
                    resource: resource.clone(),
                    simulation: *simulation,
                    model: model.clone(),
                }
            }
            Action::SetGateVoltage {
                voltage,
                compliance,
                ramp_step,
            } => {
                if self.busy() || self.pending_run {
                    return Err("Gate voltage cannot change during a measurement. Stop or wait for completion.".into());
                }
                Command::SetGateVoltage {
                    voltage: *voltage,
                    compliance: *compliance,
                    ramp_step: *ramp_step,
                }
            }
            Action::DisconnectGate => {
                if self.busy() || self.pending_run {
                    return Err("Stop measurement before disconnecting Gate.".into());
                }
                Command::DisconnectGate
            }
            Action::Claim | Action::Heartbeat => return Err("Internal control lease error".into()),
        };
        self.client.as_ref().map_err(Clone::clone)?.send(command)?;
        Ok(serde_json::json!({"accepted":true}))
    }
}
