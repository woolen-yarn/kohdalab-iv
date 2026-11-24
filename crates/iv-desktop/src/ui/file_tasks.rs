//! Native file dialogs and background measurement-copy tasks.

use super::*;

impl IvApp {
    pub(super) fn file_task_pending(&self) -> bool {
        self.pending_file_task.is_some()
    }

    pub(super) fn start_file_task(
        &mut self,
        ctx: &egui::Context,
        description: &'static str,
        blocks_close: bool,
        task: impl FnOnce() -> FileTaskResult + Send + 'static,
    ) {
        debug_assert!(self.pending_file_task.is_none());
        let (send, receive) = mpsc::sync_channel(1);
        let repaint = ctx.clone();
        thread::spawn(move || {
            let outcome = task();
            let _ = send.send(outcome);
            repaint.request_repaint();
        });
        self.pending_file_task = Some(PendingFileTask {
            description,
            result: receive,
            blocks_close,
        });
        self.report(format!("{description}…"));
    }

    pub(super) fn file_copy_in_progress(&self) -> bool {
        file_task_blocks_close(self.pending_file_task.as_ref())
    }

    pub(super) fn poll_file_task(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending_file_task.as_ref() else {
            return;
        };
        let description = pending.description;
        let outcome = match pending.result.try_recv() {
            Ok(outcome) => outcome,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.pending_file_task = None;
                self.report(format!("{description} did not complete."));
                return;
            }
        };
        self.pending_file_task = None;
        match outcome {
            FileTaskResult::Config(Some(path)) => {
                if self.busy() || self.python_locked() {
                    self.report("Configuration selection ignored because the application is busy or controlled externally.".into());
                } else {
                    self.config_path = path.display().to_string();
                    self.report("Configuration file selected.".into());
                }
            }
            FileTaskResult::OutputDirectory(Some(path)) => {
                if self.busy() || self.python_locked() {
                    self.report("Output-directory selection ignored because the application is busy or controlled externally.".into());
                } else {
                    self.settings.output_dir = path.display().to_string();
                    self.report("Output directory selected.".into());
                }
            }
            FileTaskResult::SaveDestination {
                source,
                destination,
                points,
            } => match selected_save_destination(source, destination, points) {
                Ok((source, destination, points)) => {
                    self.start_save_copy(ctx, source, destination, points)
                }
                Err(SaveCopyCompletion::Cancelled) => {
                    self.report("Save measurement copy cancelled.".into())
                }
                Err(_) => unreachable!("only cancellation is possible before copying"),
            },
            FileTaskResult::SaveCopy {
                destination,
                result,
                points,
            } => match save_copy_completion(&destination, result, points) {
                SaveCopyCompletion::Cancelled => {
                    self.report("Save measurement copy cancelled.".into())
                }
                SaveCopyCompletion::Saved(message) => {
                    self.report(message.clone());
                    self.save_notice = Some(message);
                }
                SaveCopyCompletion::Failed { log, notice } => {
                    self.report(log);
                    self.save_notice = Some(notice);
                }
            },
            FileTaskResult::Config(None) | FileTaskResult::OutputDirectory(None) => {
                self.report("Selection cancelled.".into());
            }
        }
    }

    pub(super) fn choose_config(&mut self, ctx: &egui::Context) {
        let initial = PathBuf::from(&self.config_path)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        // Start the native sheet on the main thread, then await it on a
        // worker. rfd's synchronous macOS picker calls run_modal on the
        // main thread even when invoked from a worker.
        let dialog = rfd::AsyncFileDialog::new()
            .set_title("Configuration file")
            .add_filter("JSON", &["json"])
            .set_directory(initial)
            .pick_file();
        self.start_file_task(ctx, "Configuration picker", false, move || {
            FileTaskResult::Config(wait_for_dialog(dialog).map(|file| file.path().to_path_buf()))
        });
    }

    pub(super) fn choose_output_directory(&mut self, ctx: &egui::Context) {
        let initial = PathBuf::from(&self.settings.output_dir);
        let dialog = rfd::AsyncFileDialog::new()
            .set_title("Output directory")
            .set_directory(initial)
            .pick_folder();
        self.start_file_task(ctx, "Output-directory picker", false, move || {
            FileTaskResult::OutputDirectory(
                wait_for_dialog(dialog).map(|folder| folder.path().to_path_buf()),
            )
        });
    }

    pub(super) fn start_save_copy(
        &mut self,
        ctx: &egui::Context,
        source: PathBuf,
        destination: PathBuf,
        points: usize,
    ) {
        // Register the copy as close-blocking before its worker starts. This
        // leaves no interval where a selected destination can be written as
        // the process exits.
        self.start_file_task(ctx, "Saving measurement copy", true, move || {
            let result = crate::run_export::save_run_copy(&source, &destination);
            FileTaskResult::SaveCopy {
                destination,
                result,
                points,
            }
        });
    }

    pub(super) fn save_measurement(&mut self, ctx: &egui::Context) {
        if self.file_task_pending() {
            self.save_notice =
                Some("Wait for the current file operation to finish before saving a copy.".into());
            return;
        }
        if self.busy() || self.pending_run {
            self.save_notice = Some(
                "Stop the measurement and wait for output cleanup before saving a copy.".into(),
            );
            return;
        }
        if self.state.points == 0 || self.state.path.is_empty() {
            self.save_notice = Some("No measurement data to save. Start a measurement first. Live monitor readings are not saved as sweep data.".into());
            return;
        }
        let source = PathBuf::from(&self.state.path);
        let stem = source
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("iv_run");
        let output_directory = PathBuf::from(&self.settings.output_dir);
        let default_name = format!("{stem}_copy.csv");
        let points = self.state.points;
        let dialog = rfd::AsyncFileDialog::new()
            .set_title("Save measurement copy")
            .add_filter("CSV measurement", &["csv"])
            .set_directory(output_directory)
            .set_file_name(default_name)
            .save_file();
        self.start_file_task(ctx, "Save measurement location", false, move || {
            let destination = wait_for_dialog(dialog).map(|file| file.path().to_path_buf());
            FileTaskResult::SaveDestination {
                source,
                destination,
                points,
            }
        });
    }

    pub(super) fn save_dialog(&mut self, ctx: &egui::Context) {
        let Some(message) = self.save_notice.clone() else {
            return;
        };
        let response = egui::Modal::new(egui::Id::new("save_measurement")).show(ctx, |ui| {
            ui.set_width(400.0);
            ui.heading("Save measurement");
            ui.add_space(8.0);
            ui.label(message);
            ui.add_space(12.0);
            if button(ui, "OK", 80.0, true).clicked() {
                ui.close();
            }
        });
        if response.should_close() {
            self.save_notice = None;
        }
    }
}
