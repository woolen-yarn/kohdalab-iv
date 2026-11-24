use super::*;

#[test]
fn save_copy_keeps_dots_and_recognizes_existing_csv_extension() {
    let source = PathBuf::from("run.csv");
    for (name, expected) in [
        ("sample_4.2K", "sample_4.2K.csv"),
        ("sample_4.2K.CSV", "sample_4.2K.CSV"),
        ("sample", "sample.csv"),
    ] {
        let (_, path, _) =
            selected_save_destination(source.clone(), Some(PathBuf::from(name)), 2).unwrap();
        assert_eq!(path, Path::new(expected));
    }
}

#[test]
fn dual_axis_follows_meter_data_and_keeps_saved_points_visible() {
    assert!(!dual_uses_meter_axis(false, false, false));
    assert!(dual_uses_meter_axis(true, false, false));
    assert!(!dual_uses_meter_axis(false, true, false));
    assert!(!dual_uses_meter_axis(true, true, false));
    assert!(dual_uses_meter_axis(true, true, true));
    assert!(dual_uses_meter_axis(false, true, true));
}

#[test]
fn measurement_controls_fit_narrow_and_wide_center_panes() {
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut app = IvApp::new(
        &creation,
        true,
        None,
        Arc::new(AtomicBool::new(false)),
        Instant::now(),
        None,
        None,
    )
    .unwrap();
    app.settings.filename = "sample_4.2K_long_measurement_name".into();
    for mode in [Mode::Iv, Mode::Vi] {
        app.settings.config.mode = mode;
        for width in [384.0, 400.0, 560.0, 760.0] {
            // Two passes let Grid resolve its cached column sizes.
            for _ in 0..2 {
                let _ = context.run_ui(egui::RawInput::default(), |ui| {
                    ui.set_width(width);
                    ui.set_clip_rect(egui::Rect::from_min_size(
                        ui.cursor().min,
                        Vec2::new(width, 650.0),
                    ));
                    app.measurement_controls(ui);
                });
            }
            assert!(
                context
                    .data(|d| d.get_temp::<bool>(egui::Id::new("measurement_controls_visible")))
                    .unwrap(),
                "Measurement overflow at {width} px, mode {mode:?}"
            );
            assert!(
                context
                    .data(|d| d.get_temp::<bool>(egui::Id::new("run_stop_visible")))
                    .unwrap()
            );
        }
    }
}

#[test]
fn sweep_plot_refits_on_mode_run_and_dual_chart_changes_but_keeps_current_zoom() {
    let context = egui::Context::default();
    let creation_context = eframe::CreationContext::_new_kittest(context.clone());
    let mut app = IvApp::new(
        &creation_context,
        true,
        None,
        Arc::new(AtomicBool::new(false)),
        Instant::now(),
        None,
        None,
    )
    .unwrap();
    // Use the production renderer, including its plot identity and memory.
    let render = |app: &IvApp, id: &'static str, x: f64, y: f64| {
        let samples = vec![
            (-x, -y, "forward".into(), [None, None]),
            (x, y, "forward".into(), [None, None]),
        ];
        let series = plot_series(&samples);
        let mut actual_id = egui::Id::NULL;
        let _ = context.run_ui(egui::RawInput::default(), |ui| {
            actual_id = ui.make_persistent_id(egui::Id::new(id));
            app.render_sweep_plot(
                ui,
                id,
                "Source",
                "",
                "A",
                "Voltage",
                "V",
                &samples,
                &series,
                [x, y],
                &[],
                &Default::default(),
                [0.0; 2],
                300.0,
                false,
            );
        });
        (actual_id, PlotMemory::load(&context, actual_id).unwrap())
    };
    let zoom = |id| {
        let mut memory = PlotMemory::load(&context, id).unwrap();
        memory.auto_bounds = false.into();
        memory.set_bounds(egui_plot::PlotBounds::from_min_max(
            [-1.0, -0.001],
            [1.0, 0.001],
        ));
        memory.store(&context, id);
    };
    let (id, _) = render(&app, "iv_plot", 0.0005, 1.026);
    zoom(id);
    let (_, retained) = render(&app, "iv_plot", 0.0005, 1.026);
    assert!(!retained.auto_bounds.x && !retained.auto_bounds.y);
    assert_eq!(retained.bounds().max(), [1.0, 0.001]);

    app.settings.config.mode = Mode::Vi;
    let (_, fitted) = render(&app, "iv_plot", 0.0005, 1.026);
    assert!(fitted.auto_bounds.x && fitted.auto_bounds.y);
    assert!(fitted.bounds().max()[0] < 0.001);
    assert!(fitted.bounds().max()[1] >= 1.026);

    zoom(id);
    app.run_id += 1;
    let (_, fitted) = render(&app, "iv_plot", 0.0005, 1.026);
    assert!(fitted.auto_bounds.x && fitted.auto_bounds.y);
    assert!(fitted.bounds().max()[0] < 0.001);

    app.settings.config.mode = Mode::Iv;
    let (dual_id, _) = render(&app, "dual_source_plot", 0.001, 1e-6);
    zoom(dual_id);
    render(&app, "iv_plot", 0.001, 1e-6);
    let (_, fitted) = render(&app, "dual_source_plot", 0.001, 1e-6);
    assert!(fitted.auto_bounds.x && fitted.auto_bounds.y);
    let (_, fitted) = render(&app, "dual_meter_plot", 0.0008, 1e-6);
    assert!(fitted.auto_bounds.x && fitted.auto_bounds.y);
    // Rendering the lower separate plot must not reset the upper plot's
    // independent y zoom on every frame. Linked x gestures are covered
    // through PlotUi in verify_reference_gui.py, not by editing memory
    // behind the linked group's back.
    let (upper, _) = render(&app, "separate_primary", 1e-5, 0.02);
    render(&app, "separate_reference", 1e-5, 35e-6);
    zoom(upper);
    render(&app, "separate_primary", 1e-5, 0.02);
    render(&app, "separate_reference", 1e-5, 35e-6);
    let (_, retained) = render(&app, "separate_primary", 1e-5, 0.02);
    assert!(!retained.auto_bounds.y);
    assert_eq!(retained.bounds().max()[1], 0.001);
}

#[test]
fn eframe_logic_completes_file_task_without_drawing_ui() {
    let context = egui::Context::default();
    let creation_context = eframe::CreationContext::_new_kittest(context.clone());
    let mut app = IvApp::new(
        &creation_context,
        true,
        None,
        Arc::new(AtomicBool::new(false)),
        Instant::now(),
        None,
        None,
    )
    .unwrap();
    let (send, receive) = mpsc::sync_channel(1);
    send.send(FileTaskResult::OutputDirectory(Some(PathBuf::from(
        "logic-output",
    ))))
    .unwrap();
    app.pending_file_task = Some(PendingFileTask {
        description: "Select output directory",
        result: receive,
        blocks_close: false,
    });

    let mut frame = eframe::Frame::_new_kittest();
    eframe::App::logic(&mut app, &context, &mut frame);

    assert_eq!(app.settings.output_dir, "logic-output");
    assert!(app.first_frame_s.is_none());
}

#[test]
fn eframe_logic_cancels_close_while_file_copy_is_pending_without_drawing_ui() {
    let context = egui::Context::default();
    let creation_context = eframe::CreationContext::_new_kittest(context.clone());
    let mut app = IvApp::new(
        &creation_context,
        true,
        None,
        Arc::new(AtomicBool::new(false)),
        Instant::now(),
        None,
        None,
    )
    .unwrap();
    let (_send, receive) = mpsc::sync_channel(1);
    app.pending_file_task = Some(PendingFileTask {
        description: "Saving measurement copy",
        result: receive,
        blocks_close: true,
    });
    let mut frame = eframe::Frame::_new_kittest();
    let mut input = egui::RawInput::default();
    input.viewports.insert(
        egui::ViewportId::ROOT,
        egui::ViewportInfo {
            events: vec![egui::ViewportEvent::Close],
            ..Default::default()
        },
    );

    let output = context.run_ui(input, |_| {
        eframe::App::logic(&mut app, &context, &mut frame)
    });

    assert!(app.closing);
    assert!(app.first_frame_s.is_none());
    assert!(
        output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .is_some_and(|viewport| viewport
                .commands
                .contains(&egui::ViewportCommand::CancelClose))
    );
}

#[test]
fn eframe_logic_stops_an_active_run_on_close_without_drawing_ui() {
    assert_eq!(close_action(true, false), Some(CloseAction::Stop));
    assert_eq!(close_action(true, true), Some(CloseAction::Stop));
}

#[test]
fn file_task_states_keep_picker_cancellation_and_copy_close_protection_distinct() {
    let (_send, receive) = mpsc::sync_channel(1);
    let picker = PendingFileTask {
        description: "Save measurement location",
        result: receive,
        blocks_close: false,
    };
    assert!(!file_task_blocks_close(Some(&picker)));
    assert!(selected_save_destination(PathBuf::from("run.csv"), None, 4).is_err());

    let (_send, receive) = mpsc::sync_channel(1);
    let copy = PendingFileTask {
        description: "Saving measurement copy",
        result: receive,
        blocks_close: true,
    };
    assert!(file_task_blocks_close(Some(&copy)));
    assert!(!file_task_blocks_close(None));
}

#[test]
fn save_copy_completion_preserves_success_and_failure_messages() {
    let destination = Path::new("export.csv");
    assert_eq!(
        save_copy_completion(destination, Ok(()), 7),
        SaveCopyCompletion::Saved(
            "Saved 7 points to export.csv\nThe companion .log records lifecycle status; .run.json contains settings and .raw.csv retains acquisition evidence."
                .into()
        )
    );
    assert_eq!(
        save_copy_completion(destination, Err("disk full".into()), 7),
        SaveCopyCompletion::Failed {
            log: "Save failed: disk full".into(),
            notice: "Could not save the measurement copy.\n\ndisk full\n\nThe original measurement files are unchanged.".into(),
        }
    );
}

#[test]
fn dialog_worker_can_complete_an_already_ready_native_future() {
    assert_eq!(wait_for_dialog(async { 17_u8 }), 17);
}

#[test]
fn dialog_worker_does_not_lose_a_wakeup_before_parking() {
    // Native completion can arrive between poll returning Pending and the
    // worker parking. Exercise that transition without opening a dialog.
    struct WakesDuringPoll(bool);
    impl Future for WakesDuringPoll {
        type Output = u8;
        fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u8> {
            if self.0 {
                Poll::Ready(23)
            } else {
                self.0 = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        sender
            .send(wait_for_dialog(WakesDuringPoll(false)))
            .unwrap()
    });
    assert_eq!(receiver.recv_timeout(Duration::from_secs(2)).unwrap(), 23);
}

#[test]
fn compact_cards_follow_sweep_and_dc_function_without_mixing_ranges() {
    // The meter supplies only a verified live quantity. An idle editor
    // selection is passed solely to the source card.
    let sweep = Mode::Iv;
    let dc = Some(Mode::Vi);
    assert_eq!(card_mode(sweep, false, None, dc), Mode::Vi);
    assert_eq!(card_mode(sweep, false, None, None), Mode::Iv);
    assert_eq!(card_mode(sweep, false, dc, None), Mode::Vi);
    assert_eq!(card_mode(sweep, true, dc, None), Mode::Iv);

    assert_eq!(card_mode(Mode::Iv, false, None, None), Mode::Iv);
    assert_eq!(card_mode(Mode::Vi, false, None, None), Mode::Vi);
    assert_eq!(card_mode(Mode::Iv, false, None, Some(Mode::Vi)), Mode::Vi);
    assert_eq!(
        card_mode(Mode::Iv, false, Some(Mode::Vi), Some(Mode::Iv)),
        Mode::Vi
    );
    assert_eq!(
        card_mode(Mode::Iv, true, Some(Mode::Vi), Some(Mode::Vi)),
        Mode::Iv
    );
    let mut settings = Settings::default();
    settings.config.meter_current_range = Some(0.001);
    settings.config.meter_voltage_range = Some(1.0);
    settings.current_compliance = 0.002;
    settings.voltage_compliance = 0.1;
    settings.source_voltage_protection = None;
    settings.voltage_compliance = 0.2;
    preserve_source_on_meter_edit_for_mode(&mut settings, Mode::Vi, 0.1, 0.1);
    settings.activate_mode_protection();
    assert_eq!(settings.config.mode, Mode::Iv);
    assert_eq!(settings.config.compliance, 0.002);
    assert_eq!(settings.source_voltage_protection, Some(0.1));
    assert_eq!(settings.config.meter_current_range, Some(0.001));
    assert_eq!(settings.config.meter_voltage_range, Some(1.0));
}

#[test]
fn meter_limit_edits_preserve_source_protection_in_both_modes() {
    for mode in [Mode::Iv, Mode::Vi] {
        let mut settings = Settings::default();
        settings.config.mode = mode;
        settings.current_compliance = 0.005;
        settings.voltage_compliance = 0.005;
        settings.source_current_protection = None;
        settings.source_voltage_protection = None;
        preserve_source_on_meter_edit(&mut settings, 0.001, 0.2);
        settings.activate_mode_protection();
        assert_eq!(settings.config.source_compliance, Some(0.2));
        assert_eq!(settings.config.compliance, 0.005);
        // An explicit source setting is never replaced by a meter edit.
        preserve_source_on_meter_edit(&mut settings, 0.003, 0.5);
        settings.activate_mode_protection();
        assert_eq!(settings.config.source_compliance, Some(0.2));
    }
}

#[test]
fn api_profile_uses_the_selected_models_before_plan_validation() {
    let mut profile = Settings::default();
    profile.sources[0].model = "YOKOGAWA_7651".into();
    let loaded = load_gui_profile(&serde_json::to_vec(&profile).unwrap()).unwrap();
    assert_eq!(loaded.config.source_model, "YOKOGAWA_7651");
    profile.simulation = true;
    let loaded = load_gui_profile(&serde_json::to_vec(&profile).unwrap()).unwrap();
    assert_eq!(loaded.source_model, "SIMULATED_SOURCE");
    assert_eq!(loaded.config.source_model, "YOKOGAWA_GS210");
}

#[test]
fn saved_data_inspector_has_exact_native_csv_fields_and_values() {
    assert_eq!(SNAPSHOT_FIELDS, iv_core::CSV_FIELDS);
    let point = iv_core::SweepPoint {
        target: 0.001,
        direction: "backward".into(),
        repeat_index: 2,
        sweep_index: 3,
        point_index: 4,
        total_points: 5,
    };
    let mut record = iv_core::simulate_record(&RunConfig::default(), &point, 0.5);
    record.timestamp = "2026-10-14T00:00:00Z".into();
    record.status = "running".into();
    record.run_id = "run-test".into();
    record.readings_requested = 1;
    record.readings_completed = Some(1);
    record.quality_flags = "normal".into();
    let values = record.native_csv_values();
    assert_eq!(
        values,
        [
            "run-test",
            "4",
            "2026-10-14T00:00:00Z",
            "5e-1",
            "3",
            "backward",
            "1e-3",
            "V",
            "1e-3",
            "1e-6",
            "source_readback",
            "meter",
            "1",
            "1",
            "",
            "A",
            "false",
            "normal",
            "1e-3",
            "",
            "",
            "1e-6",
            "1.0000000000000001e3",
            "",
            "1e-3",
            ""
        ]
        .map(str::to_string)
    );
}

#[test]
fn custom_point_drafts_accept_scientific_values_and_explain_invalid_entries() {
    assert_eq!(
        parse_custom_points(" -1e-3, 0,\n2.5E-3 ").unwrap(),
        vec![-0.001, 0.0, 0.0025]
    );
    for text in [
        "", "1,", "1,,2", "1,NaN", "1,inf", "1,-inf", "1,1e999", "1,text",
    ] {
        assert!(parse_custom_points(text).is_err(), "{text:?}");
    }
    assert!(
        parse_custom_points("1,")
            .unwrap_err()
            .starts_with("Point 2:")
    );
}

#[test]
fn start_failures_explain_connection_limits_and_resolution() {
    let mut config = RunConfig::default();
    assert!(
        start_block_reason(&config, false, true)
            .unwrap()
            .contains("All Connect")
    );
    assert!(
        start_block_reason(&config, true, false)
            .unwrap()
            .contains("both use simulation")
    );
    config.source_limit = 0.0001;
    assert!(
        start_block_reason(&config, true, true)
            .unwrap()
            .contains("Increase a limit only when it is safe")
    );
    config.source_limit = 0.001;
    config.start = -0.00100005;
    assert!(
        start_block_reason(&config, true, true)
            .unwrap()
            .contains("whole multiples")
    );
    config.start = -0.001;
    assert!(start_block_reason(&config, true, true).is_none());
}

#[test]
fn invalid_zero_centered_interval_explains_safe_alternatives_before_start() {
    let config = RunConfig {
        start: 0.0001,
        stop: 0.0002,
        ..RunConfig::default()
    };
    let reason = start_block_reason(&config, true, true).unwrap();
    assert!(reason.contains("must include zero"));
    assert!(reason.contains("linear or round_trip"));
    assert!(reason.contains("source will not be enabled"));
}

fn assert_route(actual: &[f64], expected: &[f64]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert!(
            (actual - expected).abs() < 1e-15,
            "{actual:e} != {expected:e}"
        );
    }
}

#[test]
fn cached_preview_shows_the_safe_negative_only_zero_centered_route() {
    let preview = sweep_preview(&RunConfig {
        start: -0.0016,
        stop: 0.0,
        step: 0.0001,
        source_limit: 0.002,
        ..RunConfig::default()
    })
    .unwrap();
    assert_eq!(preview.planned_points, 33);
    assert_route(&preview.route, &[0.0, -0.0016, 0.0]);
    assert_eq!(preview.route_text(), "Route: 0 V -> -1.6000 mV -> 0 V");
}

#[test]
fn cached_preview_preserves_asymmetric_reverse_zero_centered_order() {
    let preview = sweep_preview(&RunConfig {
        start: 0.001,
        stop: -0.002,
        step: 0.0001,
        source_limit: 0.003,
        ..RunConfig::default()
    })
    .unwrap();
    assert_eq!(preview.planned_points, 61);
    assert_route(&preview.route, &[0.0, -0.002, 0.001, 0.0]);
    assert_eq!(
        preview.route_text(),
        "Route: 0 V -> -2.0000 mV -> 1.0000 mV -> 0 V"
    );
}

#[test]
fn cached_preview_reports_custom_target_count_and_effective_step() {
    let preview = sweep_preview(&RunConfig {
        pattern: Pattern::CustomList,
        custom_points: vec![0.0001, -0.0002, 0.0003],
        step: 0.00000015,
        source_limit: 0.001,
        ..RunConfig::default()
    })
    .unwrap();
    assert_eq!(preview.planned_points, 3);
    assert!((preview.effective_step - 0.0000002).abs() < 1e-20);
    assert_route(&preview.route, &[0.0001, -0.0002, 0.0003]);
    assert_eq!(
        preview.route_text(),
        "Custom route (3 targets): 100.0000 µV -> -200.0000 µV -> 300.0000 µV"
    );
}

#[test]
fn only_implemented_instruments_are_enabled() {
    assert!(model_supported("KEYSIGHT_34411A"));
    assert!(model_supported("AGILENT_34401A"));
    assert!(model_supported("KEYSIGHT_34465A"));
    assert!(model_supported("ADCMT_7461A"));
    assert!(!model_supported("YOKOGAWA_GS200"));
    assert!(model_supported("YOKOGAWA_7651"));
}
#[test]
fn protection_popups_use_frozen_limits_and_never_claim_unverified_off() {
    let source = ProtectionAlert {
        kind: ProtectionKind::SourceCompliance,
        limit: 5.0,
        unit: "V".into(),
        observed: None,
        indicator: "H".into(),
        source_index: Some(1),
    };
    assert!(protection_role(&source));
    assert_eq!(protection_summary(&source).0, "Source compliance reached");
    assert!(
        protection_summary(&source)
            .1
            .contains("wiring and contacts")
    );
    let mut emergency = source.clone();
    emergency.indicator = "EMR".into();
    assert_eq!(
        protection_summary(&emergency).0,
        "Source protection triggered"
    );
    assert!(
        protection_summary(&emergency)
            .1
            .contains("before restarting")
    );
    let text = protection_message(&source, OutputState::Off);
    assert!(
        text.contains("Current source compliance: 5.0000 V"),
        "{text}"
    );
    assert!(text.contains("Instrument status: H"));
    assert!(text.contains("OFF verified"));
    assert!(text.contains("Source card > Device > Compliance"));
    for output in [OutputState::Unknown, OutputState::On] {
        let text = protection_message(&source, output);
        assert!(!text.contains("OFF verified"));
        assert!(text.contains("could not be verified"));
    }
    let meter = ProtectionAlert::measurement(
        &RunConfig {
            mode: Mode::Vi,
            compliance: 0.2,
            ..Default::default()
        },
        -0.3,
    );
    assert!(!protection_role(&meter));
    assert_eq!(protection_summary(&meter).0, "Meter limit reached");
    assert!(protection_summary(&meter).1.contains("review Meter limit"));
    let text = protection_message(&meter, OutputState::Off);
    assert!(text.contains("Current Meter limit:"));
    assert!(text.contains("Trigger reading: -300.0000 mV"), "{text}");
    assert!(text.contains("Meter card > Device > Limit"));
    assert!(text.contains("will not resume automatically"));
}
