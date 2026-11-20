//! Pure reference-channel analysis: linear channel fits, per-gate summaries,
//! and read-only import of gate-summary CSV files.
//!
//! Nothing here touches hardware, the UI, or writes files.

use std::path::Path;

use iv_core::Mode;

/// `(source x, meter y, label, auxiliary readings)`.
pub(super) type Sample = (f64, f64, String, [Option<f64>; 2]);

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ChannelFit {
    pub(super) resistance: Option<f64>,
    pub(super) conductance: Option<f64>,
    pub(super) offset: f64,
    pub(super) points: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct GateRun {
    pub(super) gate_voltage: Option<f64>,
    pub(super) complete: bool,
    pub(super) mode: Mode,
    pub(super) samples: Vec<Sample>,
    /// Same-record pairs: `.0` is the channel SOURCE value, `.1` the
    /// reference (gate) reading.
    pub(super) leakage: Vec<Sample>,
    pub(super) origin: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct GateSummary {
    pub(super) gate_voltage: f64,
    pub(super) fit: ChannelFit,
    pub(super) leakage: Option<f64>,
    pub(super) zero_count: usize,
    pub(super) origin: String,
}

/// Reciprocal that refuses zero / non-finite inputs and non-finite results.
fn reciprocal(value: f64) -> Option<f64> {
    if !value.is_finite() || value == 0.0 {
        return None;
    }
    let inv = 1.0 / value;
    inv.is_finite().then_some(inv)
}

/// Centered ordinary least squares `y = intercept + slope * x` with a free
/// intercept. Returns `(slope, intercept, points)`.
fn ols(pairs: &[(f64, f64)]) -> Option<(f64, f64, usize)> {
    let n = pairs.len();
    if n < 2 {
        return None;
    }
    let first_x = pairs[0].0;
    if pairs.iter().all(|&(x, _)| x == first_x) {
        return None;
    }
    let nf = n as f64;
    let mean_x = pairs.iter().map(|p| p.0).sum::<f64>() / nf;
    let mean_y = pairs.iter().map(|p| p.1).sum::<f64>() / nf;
    let (mut sxx, mut sxy) = (0.0_f64, 0.0_f64);
    for &(x, y) in pairs {
        let dx = x - mean_x;
        sxx += dx * dx;
        sxy += dx * (y - mean_y);
    }
    if sxx <= 0.0 || !sxx.is_finite() || !sxy.is_finite() {
        return None;
    }
    let slope = sxy / sxx;
    let intercept = mean_y - slope * mean_x;
    (slope.is_finite() && intercept.is_finite()).then_some((slope, intercept, n))
}

/// `Mode::Vi`: `V_meter = offset + R * I_source`.
/// `Mode::Iv`: `I_meter = offset + G * V_source`.
pub(super) fn fit_channel(mode: Mode, samples: &[Sample]) -> Option<ChannelFit> {
    let pairs: Vec<(f64, f64)> = samples
        .iter()
        .filter(|s| s.0.is_finite() && s.1.is_finite())
        .map(|s| (s.0, s.1))
        .collect();
    let (slope, offset, points) = ols(&pairs)?;
    let (resistance, conductance) = match mode {
        Mode::Vi => (Some(slope), reciprocal(slope)),
        Mode::Iv => (reciprocal(slope), Some(slope)),
    };
    Some(ChannelFit {
        resistance,
        conductance,
        offset,
        points,
    })
}

/// Mean reference reading over finite leakage samples whose source is exactly
/// 0 A. Returns `(mean, count)`; `mean` is `None` when no such sample exists.
fn zero_source_leakage(leakage: &[Sample]) -> (Option<f64>, usize) {
    let values: Vec<f64> = leakage
        .iter()
        .filter(|s| s.0 == 0.0 && s.1.is_finite())
        .map(|s| s.1)
        .collect();
    if values.is_empty() {
        return (None, 0);
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    (mean.is_finite().then_some(mean), values.len())
}

/// One summary per completed `Mode::Vi` run with explicit finite gate
/// metadata. Runs are never merged; output is stably sorted by gate voltage.
pub(super) fn summarize_runs(runs: &[GateRun]) -> Vec<GateSummary> {
    let mut out: Vec<GateSummary> = runs
        .iter()
        .filter(|run| run.complete && run.mode == Mode::Vi)
        .filter_map(|run| {
            let gate_voltage = run.gate_voltage.filter(|g| g.is_finite())?;
            let fit = fit_channel(Mode::Vi, &run.samples)?;
            let (leakage, zero_count) = zero_source_leakage(&run.leakage);
            Some(GateSummary {
                gate_voltage,
                fit,
                leakage,
                zero_count,
                origin: run.origin.clone(),
            })
        })
        .collect();
    out.sort_by(|a, b| a.gate_voltage.total_cmp(&b.gate_voltage));
    out
}

fn find_column(headers: &[String], names: &[&str]) -> Option<usize> {
    names
        .iter()
        .find_map(|name| headers.iter().position(|h| h == name))
}

fn cell(record: &csv::StringRecord, index: Option<usize>) -> &str {
    index
        .and_then(|i| record.get(i))
        .map(str::trim)
        .unwrap_or("")
}

/// Read-only import of a gate-summary CSV.
///
/// Canonical headers: `V_G,R_2T_ohm,I_gate_zero_mean_A,zero_current_samples,status`
/// (aliases `gate_voltage_V`, `resistance_ohm`, `gate_current_zero_mean_A`).
/// Gate, resistance and status columns are required; leakage, zero-count and
/// `origin` columns are optional. Only rows with status `Completed`/`completed`
/// and a finite gate are kept. The file stores no fit offset or point count,
/// so imported fits carry `offset = 0.0` and `points = 0`.
pub(super) fn read_gate_summary(path: &Path) -> Result<Vec<GateSummary>, String> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .trim(csv::Trim::All)
        .from_path(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let headers: Vec<String> = reader
        .headers()
        .map_err(|e| format!("{}: {e}", path.display()))?
        .iter()
        .map(|h| h.trim_start_matches('\u{feff}').trim().to_owned())
        .collect();

    let gate_col = find_column(&headers, &["V_G", "gate_voltage_V"]);
    let r_col = find_column(&headers, &["R_2T_ohm", "resistance_ohm"]);
    let status_col = find_column(&headers, &["status"]);
    let leak_col = find_column(
        &headers,
        &["I_gate_zero_mean_A", "gate_current_zero_mean_A"],
    );
    let zero_col = find_column(&headers, &["zero_current_samples"]);
    let origin_col = find_column(&headers, &["origin"]);

    let mut missing = Vec::new();
    if gate_col.is_none() {
        missing.push("V_G");
    }
    if r_col.is_none() {
        missing.push("R_2T_ohm");
    }
    if status_col.is_none() {
        missing.push("status");
    }
    if !missing.is_empty() {
        return Err(format!(
            "{}: missing required column(s): {}",
            path.display(),
            missing.join(", ")
        ));
    }

    let default_origin = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    let mut out = Vec::new();
    for (row, record) in reader.records().enumerate() {
        let line = row + 2;
        let record = record.map_err(|e| format!("{}: row {line}: {e}", path.display()))?;
        if !matches!(cell(&record, status_col), "Completed" | "completed") {
            continue;
        }
        let Some(gate_voltage) = cell(&record, gate_col)
            .parse::<f64>()
            .ok()
            .filter(|g| g.is_finite())
        else {
            continue;
        };
        let r_text = cell(&record, r_col);
        let resistance = r_text
            .parse::<f64>()
            .ok()
            .filter(|r| r.is_finite())
            .ok_or_else(|| {
                format!(
                    "{}: row {line}: non-finite or missing resistance {r_text:?}",
                    path.display()
                )
            })?;

        let leak_text = cell(&record, leak_col);
        let leakage = if leak_text.is_empty() {
            None
        } else {
            let value = leak_text.parse::<f64>().map_err(|_| {
                format!(
                    "{}: row {line}: invalid gate current {leak_text:?}",
                    path.display()
                )
            })?;
            value.is_finite().then_some(value)
        };

        let zero_text = cell(&record, zero_col);
        let zero_count = if zero_text.is_empty() {
            0
        } else {
            zero_text.parse::<usize>().map_err(|_| {
                format!(
                    "{}: row {line}: invalid zero_current_samples {zero_text:?}",
                    path.display()
                )
            })?
        };

        let origin = match cell(&record, origin_col) {
            "" => default_origin.clone(),
            label => label.to_owned(),
        };

        out.push(GateSummary {
            gate_voltage,
            fit: ChannelFit {
                resistance: Some(resistance),
                conductance: reciprocal(resistance),
                offset: 0.0,
                points: 0,
            },
            leakage: leakage.filter(|_| zero_count > 0),
            zero_count,
            origin,
        });
    }
    out.sort_by(|a, b| a.gate_voltage.total_cmp(&b.gate_voltage));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn s(x: f64, y: f64) -> Sample {
        (x, y, String::new(), [None, None])
    }

    /// Synthetic Vi sweep: -5..=5 uA, V = -33 mV + R * I.
    fn vi_sweep(r: f64) -> Vec<Sample> {
        (-5..=5)
            .map(|k| {
                let i = k as f64 * 1e-6;
                s(i, -0.033 + r * i)
            })
            .collect()
    }

    fn run(gate: Option<f64>, r: f64, leak: Vec<Sample>, origin: &str) -> GateRun {
        GateRun {
            gate_voltage: gate,
            complete: true,
            mode: Mode::Vi,
            samples: vi_sweep(r),
            leakage: leak,
            origin: origin.to_owned(),
        }
    }

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    fn temp_csv(name: &str, body: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "iv-reference-analysis-{}-{name}.csv",
            std::process::id()
        ));
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn vi_fit_recovers_close_resistances_with_offset() {
        for r in [1903.9, 1904.37] {
            let fit = fit_channel(Mode::Vi, &vi_sweep(r)).unwrap();
            assert!(close(fit.resistance.unwrap(), r, 1e-6), "{fit:?}");
            assert!(close(fit.offset, -0.033, 1e-12), "{fit:?}");
            assert!(close(fit.conductance.unwrap(), 1.0 / r, 1e-15));
            assert_eq!(fit.points, 11);
        }
        let a = fit_channel(Mode::Vi, &vi_sweep(1903.9)).unwrap();
        let b = fit_channel(Mode::Vi, &vi_sweep(1904.37)).unwrap();
        assert!(close(
            b.resistance.unwrap() - a.resistance.unwrap(),
            0.47,
            1e-6
        ));
    }

    #[test]
    fn iv_fit_reports_conductance_slope() {
        let g = 1.0 / 1904.37;
        let samples: Vec<Sample> = (-4..=4)
            .map(|k| {
                let v = k as f64 * 0.01;
                s(v, 2e-9 + g * v)
            })
            .collect();
        let fit = fit_channel(Mode::Iv, &samples).unwrap();
        assert!(close(fit.conductance.unwrap(), g, 1e-15));
        assert!(close(fit.resistance.unwrap(), 1904.37, 1e-6));
        assert!(close(fit.offset, 2e-9, 1e-18));
    }

    #[test]
    fn fit_rejects_degenerate_and_ignores_nonfinite() {
        assert!(fit_channel(Mode::Vi, &[]).is_none());
        assert!(fit_channel(Mode::Vi, &[s(1e-6, 0.1)]).is_none());
        assert!(fit_channel(Mode::Vi, &[s(1e-6, 0.1), s(1e-6, 0.2)]).is_none());
        let mut samples = vi_sweep(1903.9);
        samples.push(s(f64::NAN, 1.0));
        samples.push(s(1e-6, f64::INFINITY));
        let fit = fit_channel(Mode::Vi, &samples).unwrap();
        assert_eq!(fit.points, 11);
        assert!(close(fit.resistance.unwrap(), 1903.9, 1e-6));
        // Only one finite pair left -> no fit.
        assert!(fit_channel(Mode::Vi, &[s(0.0, 0.0), s(f64::NAN, 1.0)]).is_none());
    }

    #[test]
    fn zero_slope_has_no_fake_reciprocal() {
        let flat = vec![s(-1e-6, -0.033), s(0.0, -0.033), s(1e-6, -0.033)];
        let vi = fit_channel(Mode::Vi, &flat).unwrap();
        assert_eq!(vi.resistance, Some(0.0));
        assert_eq!(vi.conductance, None);
        let iv = fit_channel(Mode::Iv, &flat).unwrap();
        assert_eq!(iv.conductance, Some(0.0));
        assert_eq!(iv.resistance, None);
    }

    #[test]
    fn leakage_does_not_alter_resistance() {
        let leak_a = vec![s(0.0, 1e-12), s(0.0, 3e-12), s(1e-6, 9e-9)];
        let leak_b = vec![s(0.0, -5e-9), s(2e-6, 7e-6)];
        let a = summarize_runs(&[run(Some(1.0), 1903.9, leak_a, "a")]);
        let b = summarize_runs(&[run(Some(1.0), 1903.9, leak_b, "b")]);
        let c = summarize_runs(&[run(Some(1.0), 1903.9, Vec::new(), "c")]);
        assert_eq!(a[0].fit, b[0].fit);
        assert_eq!(a[0].fit, c[0].fit);
        // Mean only over exact 0 A source samples.
        assert!(close(a[0].leakage.unwrap(), 2e-12, 1e-24));
        assert_eq!(a[0].zero_count, 2);
        assert_eq!(b[0].leakage, Some(-5e-9));
        assert_eq!(b[0].zero_count, 1);
    }

    #[test]
    fn missing_zero_source_leaves_leakage_none() {
        let leak = vec![s(1e-6, 1e-9), s(0.0, f64::NAN), s(-1e-6, 2e-9)];
        let out = summarize_runs(&[run(Some(0.5), 1903.9, leak, "x")]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].leakage, None);
        assert_eq!(out[0].zero_count, 0);
    }

    #[test]
    fn summarize_filters_stopped_missing_gate_and_iv_and_sorts_stably() {
        let mut stopped = run(Some(-1.0), 1.0, Vec::new(), "stopped");
        stopped.complete = false;
        let mut iv = run(Some(-2.0), 1.0, Vec::new(), "iv");
        iv.mode = Mode::Iv;
        let runs = vec![
            run(Some(2.0), 1904.37, Vec::new(), "late"),
            stopped,
            run(None, 5.0, Vec::new(), "no-gate"),
            run(Some(f64::NAN), 5.0, Vec::new(), "nan-gate"),
            iv,
            run(Some(0.0), 1903.9, Vec::new(), "first"),
            run(Some(2.0), 1903.9, Vec::new(), "repeat"),
        ];
        let out = summarize_runs(&runs);
        let origins: Vec<&str> = out.iter().map(|g| g.origin.as_str()).collect();
        assert_eq!(origins, ["first", "late", "repeat"]);
        // Repeated gate runs stay separate fits.
        assert!(close(out[1].fit.resistance.unwrap(), 1904.37, 1e-6));
        assert!(close(out[2].fit.resistance.unwrap(), 1903.9, 1e-6));
    }

    #[test]
    fn imports_canonical_summary_csv() {
        let path = temp_csv(
            "canonical",
            "\u{feff}V_G,R_2T_ohm,I_gate_zero_mean_A,zero_current_samples,status\n\
             1.0,1904.37,2.5e-12,4,Completed\n\
             -1.0,1903.9,,0,completed\n\
             0.5,1800,1e-12,3,Stopped\n\
             ,1700,1e-12,3,Completed\n\
             2.0,1600,1e-12,3,\n\
             0.0,0,,,Completed\n",
        );
        let out = read_gate_summary(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].gate_voltage, -1.0);
        assert_eq!(out[0].fit.resistance, Some(1903.9));
        assert_eq!(out[0].leakage, None);
        assert_eq!(out[0].zero_count, 0);
        assert_eq!(out[1].gate_voltage, 0.0);
        assert_eq!(out[1].fit.conductance, None);
        assert_eq!(out[2].fit.resistance, Some(1904.37));
        assert!(close(out[2].fit.conductance.unwrap(), 1.0 / 1904.37, 1e-18));
        assert_eq!(out[2].leakage, Some(2.5e-12));
        assert_eq!(out[2].zero_count, 4);
        assert!(out[2].origin.ends_with("canonical.csv"));
    }

    #[test]
    fn imports_alias_headers_with_origin_label() {
        let path = temp_csv(
            "alias",
            "status,origin,gate_voltage_V,resistance_ohm,gate_current_zero_mean_A\n\
             Completed,run-7,0.25,1903.9,-1e-11\n",
        );
        let out = read_gate_summary(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].gate_voltage, 0.25);
        assert_eq!(out[0].leakage, None);
        assert_eq!(out[0].zero_count, 0);
        assert_eq!(out[0].origin, "run-7");
    }

    #[test]
    fn import_rejects_missing_headers_and_nonfinite_fit() {
        let path = temp_csv("noheader", "V_G,status\n1.0,Completed\n");
        let err = read_gate_summary(&path).unwrap_err();
        std::fs::remove_file(&path).ok();
        assert!(err.contains("R_2T_ohm"), "{err}");

        let path = temp_csv("nostatus", "V_G,R_2T_ohm\n1.0,1903.9\n");
        let err = read_gate_summary(&path).unwrap_err();
        std::fs::remove_file(&path).ok();
        assert!(err.contains("status"), "{err}");

        for bad in ["NaN", "inf", "", "abc"] {
            let path = temp_csv(
                "nonfinite",
                &format!("V_G,R_2T_ohm,status\n1.0,{bad},Completed\n"),
            );
            let result = read_gate_summary(&path);
            std::fs::remove_file(&path).ok();
            assert!(result.is_err(), "{bad:?} accepted");
        }

        // Incomplete rows are omitted before the resistance is checked.
        let path = temp_csv("stopped-nan", "V_G,R_2T_ohm,status\n1.0,NaN,Stopped\n");
        let out = read_gate_summary(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert!(out.is_empty());
    }
}

#[cfg(test)]
mod missing_zero_tests {
    use super::*;
    #[test]
    fn imported_leakage_requires_positive_explicit_zero_count() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("summary.csv");
        std::fs::write(&path,"V_G,R_2T_ohm,I_gate_zero_mean_A,zero_current_samples,status\n-1,1904,-3.5e-5,0,completed\n0,1904,-3.5e-5,,completed\n1,1904,-3.5e-5,2,completed\n").unwrap();
        let rows = read_gate_summary(&path).unwrap();
        assert_eq!(rows.len(), 3);
        assert!(rows[0].leakage.is_none());
        assert!(rows[1].leakage.is_none());
        assert_eq!(rows[2].leakage, Some(-3.5e-5));
    }
}
