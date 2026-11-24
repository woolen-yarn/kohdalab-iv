//! Engine command dispatch, snapshot synchronization and run start.

use super::*;

impl IvApp {
    pub(super) fn send(&mut self, command: Command) -> bool {
        if self.reference_replay {
            return false;
        }
        let result = self
            .client
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|c| c.send(command));
        if let Err(error) = result {
            self.report(error);
            false
        } else {
            true
        }
    }

    pub(super) fn busy(&self) -> bool {
        self.pending_dc.is_some()
            || self.pending_inventory_selection.is_some()
            || self.pending_bindings.is_some()
            || matches!(
                self.state.phase,
                Phase::Connecting
                    | Phase::Scanning
                    | Phase::SwitchingMode
                    | Phase::Preparing
                    | Phase::Running
                    | Phase::CleaningUp
            )
    }

    pub(super) fn active(&self) -> bool {
        self.pending_dc.is_some()
            || matches!(
                self.state.phase,
                Phase::Preparing | Phase::Running | Phase::CleaningUp
            )
    }

    pub(super) fn sync(&mut self) {
        self.settings.config.source_model = if self.settings.source_model == "SIMULATED_SOURCE" {
            "YOKOGAWA_GS210".into()
        } else {
            self.settings.source_model.clone()
        };
        self.settings.config.meter_model = if self.settings.meter_model == "SIMULATED_METER" {
            "AGILENT_34411A".into()
        } else {
            self.settings.meter_model.clone()
        };
        if let Some(dual) = &mut self.settings.config.balanced_source {
            dual.source_model = if self.settings.source2_model == "SIMULATED_SOURCE" {
                "YOKOGAWA_GS210".into()
            } else {
                self.settings.source2_model.clone()
            };
        }
        let config_changed = self.settings.config != self.validated_config;
        if config_changed {
            self.preview = sweep_preview(&self.settings.config);
            self.validated_config = self.settings.config.clone();
        }
        let mut state_changed = false;
        if let Ok(client) = &self.client {
            let Some(next) = client.snapshot_since(&mut self.state_revision) else {
                self.refresh_table_values(config_changed, false);
                return;
            };
            state_changed = true;
            dc::settle_job(&mut self.pending_dc, &mut self.start_error, &next);
            self.pending_inventory_dc.retain(|device, (id, safety)| {
                next.safety_revision == *safety
                    && next
                        .inventory_devices
                        .iter()
                        .any(|entry| entry.id == *device && entry.dc_job.as_ref() != Some(id))
                    && !next.gui_jobs.iter().any(|job| &job.id == id)
            });
            if next.balanced_active != self.state.balanced_active {
                self.live_samples.clear();
                self.source2_samples.clear();
                self.last_live_timestamp.clear();
            }
            match (&self.state.idle_reading, &next.idle_reading) {
                (_, None) => self.idle_samples.clear(),
                (previous, Some(reading)) => {
                    if previous
                        .as_ref()
                        .is_none_or(|old| old.mode != reading.mode || old.source != reading.source)
                    {
                        self.idle_samples.clear();
                    }
                    if previous
                        .as_ref()
                        .is_none_or(|old| old.timestamp != reading.timestamp)
                    {
                        let time = self.boot.elapsed().as_secs_f64();
                        self.idle_samples.push_back([time, reading.value]);
                        while self.idle_samples.len() > 1024
                            || self
                                .idle_samples
                                .front()
                                .is_some_and(|sample| time - sample[0] > 30.0)
                        {
                            self.idle_samples.pop_front();
                        }
                    }
                }
            }
            if next.source_connected != self.state.source_connected
                || next.meter_connected != self.state.meter_connected
            {
                self.live_samples.clear();
                self.live_reading = None;
            }
            if next.reference_meter_connected != self.state.reference_meter_connected
                || next.reference_meter_resource != self.state.reference_meter_resource
                || next
                    .live
                    .as_ref()
                    .zip(self.live_reading.as_ref())
                    .is_some_and(|(new, old)| new.mode != old.mode)
            {
                self.reference_live_samples.clear();
            }
            if let Some(record) = &next.live
                && self.last_live_timestamp != record.timestamp
            {
                if self
                    .live_reading
                    .as_ref()
                    .is_some_and(|old| old.mode != record.mode)
                {
                    self.live_samples.clear();
                }
                let mut sample = record.clone();
                sample.elapsed_s = self.boot.elapsed().as_secs_f64();
                append_live_sample(&mut self.live_samples, &sample);
                if let Some(value) = record.reference_meter_value {
                    self.reference_live_samples
                        .push_back([sample.elapsed_s, value]);
                    if self.reference_live_samples.len() > 1024 {
                        self.reference_live_samples.pop_front();
                    }
                }
                if let Some(level) = next.source2_level {
                    let mut secondary = sample.clone();
                    secondary.source_set = level;
                    secondary.source_readback = Some(level);
                    secondary.mode = Mode::Iv;
                    append_live_sample(&mut self.source2_samples, &secondary);
                }
                self.last_live_timestamp.clone_from(&record.timestamp);
                self.live_reading = Some(record.clone());
            }
            let new_run = next.run_id > self.run_id;
            if new_run {
                self.run_id = next.run_id;
                self.samples.clear();
                self.plot_series.iter_mut().for_each(Vec::clear);
                self.plot_extent = [0.0; 2];
                self.reference_samples.clear();
                self.separate = Default::default();
                self.gate_metric = reference_view::GateMetric::Sweep;
                self.reference_plot_series.iter_mut().for_each(Vec::clear);
                self.reference_plot_extent = [0.0; 2];
                self.notice.clear();
            }
            let new_points = client.drain_points();
            self.sync_inventory(&next);
            for (record_run, point) in new_points {
                if record_run < self.run_id {
                    continue;
                }
                if record_run > self.run_id {
                    self.samples.clear();
                    self.plot_series.iter_mut().for_each(Vec::clear);
                    self.plot_extent = [0.0; 2];
                    self.reference_samples.clear();
                    self.separate = Default::default();
                    self.gate_metric = reference_view::GateMetric::Sweep;
                    self.reference_plot_series.iter_mut().for_each(Vec::clear);
                    self.reference_plot_extent = [0.0; 2];
                    self.run_id = record_run;
                }
                self.separate.push(&point);
                let source = point.source_readback.unwrap_or(point.source_set);
                let xy = (source, point.meter_value);
                extend_plot_extent(&mut self.plot_extent, xy.0, xy.1);
                append_plot_point(
                    &mut self.plot_series,
                    self.samples.last().map(|sample| sample.2.as_str()),
                    xy.0,
                    xy.1,
                    &point.point.direction,
                );
                self.samples.push((
                    xy.0,
                    xy.1,
                    point.point.direction.clone(),
                    [point.source_resolution, point.meter_resolution],
                ));
                if let Some(reference) = point.reference_meter_value {
                    extend_plot_extent(
                        &mut self.reference_plot_extent,
                        reference,
                        point.meter_value,
                    );
                    append_plot_point(
                        &mut self.reference_plot_series,
                        self.reference_samples
                            .last()
                            .map(|sample| sample.2.as_str()),
                        reference,
                        point.meter_value,
                        &point.point.direction,
                    );
                    self.reference_samples.push((
                        reference,
                        point.meter_value,
                        point.point.direction.clone(),
                        [point.reference_meter_resolution, point.meter_resolution],
                    ));
                }
            }
            if self.samples.len() > 50000 {
                let mut reduced = Vec::with_capacity(16000);
                for chunk in self.samples.chunks(8) {
                    let low = chunk
                        .iter()
                        .enumerate()
                        .min_by(|a, b| a.1.1.total_cmp(&b.1.1))
                        .map(|x| x.0)
                        .unwrap_or(0);
                    let high = chunk
                        .iter()
                        .enumerate()
                        .max_by(|a, b| a.1.1.total_cmp(&b.1.1))
                        .map(|x| x.0)
                        .unwrap_or(0);
                    for index in 0..chunk.len() {
                        if index == low
                            || index == high
                            || (index > 0 && chunk[index].2 != chunk[index - 1].2)
                        {
                            reduced.push(chunk[index].clone());
                        }
                    }
                }
                if reduced.len() >= self.samples.len() {
                    reduced = self.samples.iter().step_by(2).cloned().collect();
                }
                self.samples = reduced;
                self.rebuild_plot_series();
            }
            if self.reference_samples.len() > 50000 {
                self.reference_samples =
                    self.reference_samples.iter().step_by(2).cloned().collect();
                self.rebuild_reference_plot_series();
            }
            if Some(next.phase) != self.previous_phase {
                self.phase_start = Instant::now();
                self.previous_phase = Some(next.phase);
            }
            if !next.error.is_empty()
                && next.error != self.state.error
                && (self.pending_run
                    || self.active()
                    || self.state.source_dc.is_some()
                    || self.state.source2_dc.is_some()
                    || self.state.gate_output == OutputState::On)
            {
                self.start_error = Some(format!(
                    "{}\n\nCheck the log and instrument connections. Verify output is off before trying again.",
                    next.error
                ));
            }
            if next.protection_alert.is_some()
                && (next.protection_alert != self.state.protection_alert
                    || next.run_id != self.state.run_id)
            {
                self.start_error = None;
                self.protection_dialog = next.protection_alert.clone();
            }
            let log = if next.error.is_empty() {
                next.message.clone()
            } else {
                format!("{}\n{}", next.message, next.error)
            };
            if log != self.previous_log {
                self.previous_log = log.clone();
                self.logs.push(log);
                if self.logs.len() > 200 {
                    self.logs.remove(0);
                }
            }
            if self.pending_run
                && next.run_id <= self.pending_base
                && next.phase != Phase::Failed
                && next.safety_revision <= self.pending_safety
            {
                self.state.phase = Phase::Preparing;
                self.state.message = "Sending the validated measurement plan…".into();
            } else if next.run_id >= self.run_id {
                self.pending_run = false;
                self.state = next;
            }
        }
        self.refresh_table_values(config_changed, state_changed);
    }

    pub(super) fn rebuild_plot_series(&mut self) {
        self.plot_series = plot_series(&self.samples);
        self.plot_extent = [0.0; 2];
        for (x, y, _, _) in &self.samples {
            extend_plot_extent(&mut self.plot_extent, *x, *y);
        }
    }

    pub(super) fn rebuild_reference_plot_series(&mut self) {
        self.reference_plot_series = plot_series(&self.reference_samples);
        self.reference_plot_extent = [0.0; 2];
        for (x, y, _, _) in &self.reference_samples {
            extend_plot_extent(&mut self.reference_plot_extent, *x, *y);
        }
    }

    pub(super) fn refresh_table_values(&mut self, config_changed: bool, state_changed: bool) {
        if !config_changed
            && (!state_changed
                || (self.table_run_id == self.state.run_id
                    && self.table_points == self.state.points))
        {
            return;
        }
        self.table_values = self
            .state
            .last
            .as_ref()
            .map(|record| {
                let mut values = Vec::from(record.native_csv_values());
                if let Some(levels) = record.dual_source_levels {
                    values.extend(levels.csv_values());
                }
                values
            })
            .unwrap_or_default();
        self.table_run_id = self.state.run_id;
        self.table_points = self.state.points;
    }

    pub(super) fn start_run(&mut self) {
        self.start_run_impl(true);
    }

    pub(super) fn start_run_impl(&mut self, save_preferences: bool) {
        if self.pending_run || self.file_task_pending() {
            return;
        }
        if !self.state.inventory_outputs_managed() {
            let reason = crate::protocol::UNASSIGNED_OUTPUT_BLOCK.to_owned();
            self.report(reason.clone());
            self.start_error = Some(reason);
            return;
        }
        if let Some(reason) = start_block_reason(
            &self.settings.config,
            self.state.connected
                && (self.settings.config.balanced_source.is_none() || self.state.source2_connected),
            self.state.source_simulation == self.state.meter_simulation
                && (self.settings.config.balanced_source.is_none()
                    || self.state.source_simulation == self.state.source2_simulation),
        ) {
            self.report(reason.clone());
            self.start_error = Some(reason);
            return;
        }
        let basename = match control::api_filename(&self.settings.filename) {
            Ok(basename) => basename,
            Err(error) => {
                self.report(error.clone());
                self.start_error = Some(error);
                return;
            }
        };
        let directory = PathBuf::from(&self.settings.output_dir);
        let stamp = time::OffsetDateTime::now_utc().unix_timestamp_nanos();
        let path = directory.join(if self.settings.auto_suffix {
            format!("{basename}_{stamp}.csv")
        } else {
            format!("{basename}.csv")
        });
        if !self.smoke && save_preferences {
            self.settings.store_active_entries();
            self.settings.simulation = self.both_simulated();
            if let Err(error) = save_settings(&self.settings, Path::new(&self.config_path)) {
                self.report(format!("Settings were not saved: {error}"));
            }
        }
        if !self.send(Command::Run {
            config: self.settings.config.clone(),
            path: path.display().to_string(),
        }) {
            self.start_error = Some("The measurement request could not be sent. Check the log and reconnect the instruments before trying again.".into());
            return;
        }
        self.protection_dialog = None;
        self.start_error = None;
        self.state.error.clear();
        self.pending_run = true;
        self.pending_base = self.state.run_id;
        self.pending_safety = self.state.safety_revision;
        self.state.phase = Phase::Preparing;
    }

    pub(super) fn both_simulated(&self) -> bool {
        self.settings.source_model == "SIMULATED_SOURCE"
            && self.settings.meter_model == "SIMULATED_METER"
            && (self.settings.config.balanced_source.is_none()
                || self.settings.source2_model == "SIMULATED_SOURCE")
    }

    pub(super) fn report(&mut self, message: String) {
        self.notice = message.clone();
        self.logs.push(message);
        if self.logs.len() > 200 {
            self.logs.remove(0);
        }
    }
}
