//! Compatibility snapshot including the ADCMT 7461A GPIB DCV specification.
//!
//! The historical c741030 snapshot is retained. The 7461A snapshot adds only
//! the intentional GPIB DCV capability and validation changes.
//!
//! Covered public functions: `source_display_resolution`, `source_range_query`,
//! `source_protection_profile`, `manual_source_range`, `fixed_manual_source_range`,
//! `meter_ranges`, `meter_display_queries`, `meter_range_resistances`,
//! `meter_input_resistance` and `validate_meter_range`, for every registered model
//! (enumerated through `instrument_catalog` / `instrument_profile`), both modes, and
//! the rejection results for invalid inputs and unregistered or wrong-role models.
use std::fmt::Write as _;

use iv_core::{
    InstrumentProfile, InstrumentRole, Mode, RunConfig, SourceLimitProfile, SourceProfile,
    fixed_manual_source_range, instrument_catalog, instrument_profile, manual_source_range,
    meter_display_queries, meter_input_resistance, meter_range_resistances, meter_ranges,
    source_display_resolution, source_protection_profile, source_range_query, validate_meter_range,
};

const GOLDEN: &str = include_str!("../../../tests/golden/spec_7461a_gpib_dcv.txt");

/// Model ids that must be rejected or reported as unsupported by every function.
const UNREGISTERED_MODELS: [&str; 3] = ["", "UNKNOWN_MODEL", "yokogawa_gs210"];

const MODES: [Mode; 2] = [Mode::Iv, Mode::Vi];

fn source_level_cases(range_maximum: f64, resolution: f64) -> [(&'static str, f64); 11] {
    [
        ("0", 0.0),
        ("-resolution", -resolution),
        ("+resolution", resolution),
        ("+0.5*resolution", 0.5 * resolution),
        ("+1.5*resolution", 1.5 * resolution),
        ("-maximum", -range_maximum),
        ("+maximum", range_maximum),
        ("-(maximum+resolution)", -(range_maximum + resolution)),
        ("+(maximum+resolution)", range_maximum + resolution),
        ("+inf", f64::INFINITY),
        ("NaN", f64::NAN),
    ]
}

fn limit_cases(limit: &SourceLimitProfile) -> Vec<(&'static str, f64)> {
    let step = limit.resolution.unwrap_or(limit.minimum);
    vec![
        ("0", 0.0),
        ("-maximum", -limit.maximum),
        ("0.5*minimum", 0.5 * limit.minimum),
        ("minimum", limit.minimum),
        ("maximum", limit.maximum),
        ("maximum+resolution", limit.maximum + step),
        ("+inf", f64::INFINITY),
        ("NaN", f64::NAN),
    ]
}

fn source_cases(snapshot: &mut String, model: &str, mode: Mode, profile: &SourceProfile) {
    // Voltage source (IV) is protected by a current limit and vice versa.
    let (ranges, limit) = match mode {
        Mode::Iv => (&profile.voltage_ranges, &profile.current_limit),
        Mode::Vi => (&profile.current_ranges, &profile.voltage_limit),
    };

    for range in ranges {
        let command = range.command_value;
        let display_resolution = source_display_resolution(model, mode, command);
        writeln!(
            snapshot,
            "range {command:?} ({:?}) display_resolution={display_resolution:?}",
            range.name
        )
        .unwrap();

        for (label, level) in source_level_cases(range.maximum, range.resolution) {
            let automatic = manual_source_range(model, mode, level, limit.maximum);
            let fixed = fixed_manual_source_range(model, mode, level, limit.maximum, command);
            writeln!(
                snapshot,
                "  level {label}={level:?}: automatic={automatic:?} fixed={fixed:?}"
            )
            .unwrap();
        }
        // Protection limit handling, including divider ranges with a fixed hardware limit.
        for (label, compliance) in limit_cases(limit) {
            let automatic = manual_source_range(model, mode, range.resolution, compliance);
            let fixed = fixed_manual_source_range(model, mode, 0.0, compliance, command);
            writeln!(
                snapshot,
                "  limit {label}={compliance:?}: automatic={automatic:?} fixed={fixed:?}"
            )
            .unwrap();
        }
    }

    // Range selections that are not registered command values.
    let largest = ranges.iter().map(|r| r.command_value).fold(0.0, f64::max);
    for (label, command) in [
        ("0", 0.0),
        ("-largest", -largest),
        ("largest*1.5", largest * 1.5),
        ("largest+1e-6*largest", largest * (1.0 + 1e-6)),
        ("+inf", f64::INFINITY),
        ("NaN", f64::NAN),
    ] {
        let display = source_display_resolution(model, mode, command);
        let fixed = fixed_manual_source_range(model, mode, 0.0, limit.maximum, command);
        writeln!(
            snapshot,
            "invalid source range {label}={command:?}: display={display:?} fixed={fixed:?}"
        )
        .unwrap();
    }
}

fn meter_config(model: &str, mode: Mode) -> RunConfig {
    RunConfig {
        meter_model: model.into(),
        mode,
        meter_current_range: None,
        meter_voltage_range: None,
        ..RunConfig::default()
    }
}

fn set_meter_range(config: &mut RunConfig, range: Option<f64>) {
    match config.mode {
        Mode::Iv => config.meter_current_range = range,
        Mode::Vi => config.meter_voltage_range = range,
    }
}

fn set_inactive_meter_range(config: &mut RunConfig, range: Option<f64>) {
    match config.mode {
        Mode::Iv => config.meter_voltage_range = range,
        Mode::Vi => config.meter_current_range = range,
    }
}

fn meter_cases(snapshot: &mut String, model: &str, mode: Mode, maximum: f64) {
    let ranges = meter_ranges(model, mode);
    let queries = meter_display_queries(model, mode);
    let resistances = meter_range_resistances(model, mode);
    writeln!(
        snapshot,
        "ranges={ranges:?} display_queries={queries:?} range_resistances={resistances:?}"
    )
    .unwrap();

    let mut config = meter_config(model, mode);
    writeln!(snapshot, "auto={:?}", validate_meter_range(&config)).unwrap();
    // Only the range for the measured quantity is relevant; the inactive one is kept.
    set_inactive_meter_range(&mut config, Some(f64::NAN));
    writeln!(
        snapshot,
        "auto with invalid inactive range={:?}",
        validate_meter_range(&config)
    )
    .unwrap();
    set_inactive_meter_range(&mut config, None);

    // Meters take a range in SI units; no setting-resolution grid is declared for it.
    // Probe every fixed range, every loading-only range (notably the 7461A's Auto-only
    // current inputs), near-matches around the comparison tolerance and invalid values.
    let ranges = ranges.unwrap_or_default();
    let mut probes: Vec<(String, f64)> = Vec::new();
    for range in ranges
        .iter()
        .copied()
        .chain(resistances.iter().map(|row| row.range))
    {
        probes.push(("listed".into(), range));
        probes.push(("listed*(1+1e-13)".into(), range * (1.0 + 1e-13)));
        probes.push(("listed*(1+1e-9)".into(), range * (1.0 + 1e-9)));
    }
    let largest = ranges.iter().copied().fold(0.0, f64::max);
    probes.extend([
        ("0".into(), 0.0),
        ("-0".into(), -0.0),
        ("-maximum_input".into(), -maximum),
        ("maximum_input".into(), maximum),
        ("maximum_input*1.01".into(), maximum * 1.01),
        ("largest_fixed*10".into(), largest * 10.0),
        ("+inf".into(), f64::INFINITY),
        ("-inf".into(), f64::NEG_INFINITY),
        ("NaN".into(), f64::NAN),
    ]);
    if let Some(&smallest) = ranges.iter().min_by(|a, b| a.total_cmp(b)) {
        probes.push(("-smallest_fixed".into(), -smallest));
        probes.push(("smallest_fixed/10".into(), smallest / 10.0));
    }

    for (label, range) in probes {
        set_meter_range(&mut config, Some(range));
        let validation = validate_meter_range(&config);
        let resistance = meter_input_resistance(model, mode, range);
        writeln!(
            snapshot,
            "range {label}={range:?}: validation={validation:?} resistance={resistance:?}"
        )
        .unwrap();
    }
}

fn unregistered_and_wrong_role_cases(snapshot: &mut String) {
    let sources = instrument_catalog(InstrumentRole::Source);
    let meters = instrument_catalog(InstrumentRole::Meter);
    let meter_ids = meters.iter().map(|d| d.id);
    let source_ids = sources.iter().map(|d| d.id);

    writeln!(
        snapshot,
        "\n== source functions with unregistered or meter ids =="
    )
    .unwrap();
    for model in UNREGISTERED_MODELS.into_iter().chain(meter_ids) {
        writeln!(
            snapshot,
            "model {model:?}: profile={:?} range_query={:?} protection={:?}",
            instrument_profile(model).map(|p| p.is_some()),
            source_range_query(model),
            source_protection_profile(model)
        )
        .unwrap();
        for mode in MODES {
            writeln!(
                snapshot,
                "  mode={mode:?}: display={:?} automatic={:?} fixed={:?}",
                source_display_resolution(model, mode, 1.0),
                manual_source_range(model, mode, 0.0, 0.01),
                fixed_manual_source_range(model, mode, 0.0, 0.01, 1.0)
            )
            .unwrap();
        }
    }

    writeln!(
        snapshot,
        "\n== meter functions with unregistered or source ids =="
    )
    .unwrap();
    for model in UNREGISTERED_MODELS.into_iter().chain(source_ids) {
        writeln!(
            snapshot,
            "model {model:?}: profile={:?}",
            instrument_profile(model).map(|p| p.is_some())
        )
        .unwrap();
        for mode in MODES {
            let mut config = meter_config(model, mode);
            let auto = validate_meter_range(&config);
            set_meter_range(&mut config, Some(1.0));
            let fixed = validate_meter_range(&config);
            writeln!(
                snapshot,
                "  mode={mode:?}: ranges={:?} display_queries={:?} range_resistances={:?} \
                 resistance={:?} auto={auto:?} fixed={fixed:?}",
                meter_ranges(model, mode),
                meter_display_queries(model, mode),
                meter_range_resistances(model, mode),
                meter_input_resistance(model, mode, 1.0)
            )
            .unwrap();
        }
    }
}

fn snapshot() -> String {
    let mut snapshot = String::new();
    for role in [InstrumentRole::Source, InstrumentRole::Meter] {
        for descriptor in instrument_catalog(role) {
            let model = descriptor.id;
            let profile = instrument_profile(model)
                .unwrap_or_else(|e| panic!("{model}: {e}"))
                .unwrap_or_else(|| panic!("{model}: registered model has no profile"));
            writeln!(
                snapshot,
                "\n== {model} ==\n{descriptor:?}\nprofile={profile:?}"
            )
            .unwrap();
            if role == InstrumentRole::Source {
                let query = source_range_query(model);
                let protection = source_protection_profile(model);
                writeln!(snapshot, "range_query={query:?} protection={protection:?}").unwrap();
            }

            for mode in MODES {
                writeln!(snapshot, "-- mode={mode:?} --").unwrap();
                match &profile {
                    InstrumentProfile::Source(source) => {
                        source_cases(&mut snapshot, model, mode, source);
                    }
                    InstrumentProfile::Meter(meter) => {
                        let maximum = match mode {
                            Mode::Iv => meter.maximum_input_current,
                            Mode::Vi => meter.maximum_input_voltage,
                        };
                        meter_cases(&mut snapshot, model, mode, maximum);
                    }
                }
            }
        }
    }
    unregistered_and_wrong_role_cases(&mut snapshot);
    snapshot
}

#[test]
fn model_spec_matches_7461a_gpib_dcv_snapshot() {
    let actual = snapshot();
    // Tolerate CRLF checkouts of the golden file.
    let expected = GOLDEN.replace("\r\n", "\n");
    if actual == expected {
        return;
    }
    let mut expected_lines = expected.lines();
    for (index, line) in actual.lines().enumerate() {
        let golden = expected_lines.next();
        assert_eq!(
            Some(line),
            golden,
            "specification snapshot differs from tests/golden/spec_7461a_gpib_dcv.txt at line {}",
            index + 1
        );
    }
    panic!(
        "specification snapshot differs from tests/golden/spec_7461a_gpib_dcv.txt: \
         {} trailing golden line(s) were not produced (or a trailing newline differs)",
        expected_lines.count()
    );
}
