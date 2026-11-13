//! Bounded live history and cached sweep drawing series.
//! Turning-point anchors and line breaks are drawing data, not CSV samples.

use egui_plot::PlotPoint;
use iv_core::Mode;
use std::collections::VecDeque;

pub(super) fn extend_plot_extent(extent: &mut [f64; 2], x: f64, y: f64) {
    for (maximum, value) in extent.iter_mut().zip([x, y]) {
        if value.is_finite() {
            *maximum = maximum.max(value.abs());
        }
    }
}

pub(super) fn plot_series(samples: &[(f64, f64, String, [Option<f64>; 2])]) -> [Vec<PlotPoint>; 3] {
    let mut series = std::array::from_fn(|_| Vec::new());
    let mut previous = None;
    for (x, y, direction, _) in samples {
        append_plot_point(&mut series, previous, *x, *y, direction);
        previous = Some(direction.as_str());
    }
    series
}

pub(super) fn append_plot_point(
    series: &mut [Vec<PlotPoint>; 3],
    previous: Option<&str>,
    x: f64,
    y: f64,
    direction: &str,
) {
    let index = |direction: &str| match direction {
        "forward" => 0,
        "backward" => 1,
        _ => 2,
    };
    let current = index(direction);
    if let Some(last) = previous.map(index).filter(|last| *last != current) {
        // Share the measured turning point across the two colored segments.
        // This is a drawing anchor only; it does not create another CSV sample.
        if last < 2
            && current < 2
            && let Some(anchor) = series[last].last().copied()
            && anchor.x.is_finite()
            && anchor.y.is_finite()
        {
            series[current].push(anchor);
        }
        series[last].push(PlotPoint::new(f64::NAN, f64::NAN));
    }
    series[current].push(PlotPoint::new(x, y));
}

#[derive(Clone, Copy)]
pub(super) struct LiveSample {
    pub(super) time: f64,
    pub(super) voltage: f64,
    pub(super) current: f64,
}

pub(super) fn append_live_sample(samples: &mut VecDeque<LiveSample>, record: &iv_core::Record) {
    let source = record.source_readback.unwrap_or(record.source_set);
    let (voltage, current) = if record.mode == Mode::Iv {
        (source, record.meter_value)
    } else {
        (record.meter_value, source)
    };
    if ![record.elapsed_s, voltage, current]
        .into_iter()
        .all(f64::is_finite)
    {
        return;
    }
    samples.push_back(LiveSample {
        time: record.elapsed_s,
        voltage,
        current,
    });
    while samples.len() > 1024
        || samples
            .front()
            .is_some_and(|s| record.elapsed_s - s.time > 30.0)
    {
        samples.pop_front();
    }
}

#[cfg(test)]
mod tests {
    use super::super::formatting::AxisScale;
    use super::*;

    #[test]
    fn live_history_is_bounded_and_iv_vi_values_keep_their_channel_units() {
        let mut samples = VecDeque::new();
        let config = iv_core::RunConfig::default();
        let point = iv_core::SweepPoint {
            target: 0.001,
            direction: "forward".into(),
            repeat_index: 1,
            sweep_index: 1,
            point_index: 1,
            total_points: 1,
        };
        for i in 0..2000 {
            append_live_sample(
                &mut samples,
                &iv_core::simulate_record(&config, &point, i as f64 * 0.01),
            );
        }
        assert_eq!(samples.len(), 1024);
        assert_eq!(samples.back().unwrap().voltage, 0.001);
        assert_eq!(samples.back().unwrap().current, 1e-6);
        let mut record = iv_core::simulate_record(
            &iv_core::RunConfig {
                mode: Mode::Vi,
                ..config
            },
            &point,
            50.0,
        );
        append_live_sample(&mut samples, &record);
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].current, 0.001);
        assert_eq!(samples[0].voltage, 1.0);
        record.meter_value = f64::NAN;
        append_live_sample(&mut samples, &record);
        assert_eq!(samples.len(), 1);
    }

    #[test]
    fn axis_extent_ignores_breaks_and_keeps_plot_coordinates_in_si() {
        let samples = vec![
            (-0.001, -1e-6, "forward".into(), [None; 2]),
            (0.001, 1e-6, "backward".into(), [None; 2]),
        ];
        let mut extent = [0.0; 2];
        for (x, y, _, _) in &samples {
            extend_plot_extent(&mut extent, *x, *y);
        }
        extend_plot_extent(&mut extent, f64::NAN, f64::INFINITY);
        assert_eq!(extent, [0.001, 1e-6]);
        let plotted = plot_series(&samples);
        assert_eq!(plotted[0][0], PlotPoint::new(-0.001, -1e-6));
        assert_eq!(plotted[1][0], plotted[0][0]);
        assert_eq!(plotted[1][1], PlotPoint::new(0.001, 1e-6));
        assert_eq!(AxisScale::for_magnitude(extent[0]).prefix, "m");
        assert_eq!(AxisScale::for_magnitude(extent[1]).prefix, "µ");
    }

    #[test]
    fn cached_plot_series_preserves_direction_breaks() {
        let samples = vec![
            (0.0, 1.0, "forward".into(), [None; 2]),
            (1.0, 2.0, "backward".into(), [None; 2]),
            (2.0, 3.0, "forward".into(), [None; 2]),
            (3.0, 4.0, "custom-a".into(), [None; 2]),
            (4.0, 5.0, "custom-b".into(), [None; 2]),
        ];
        let series = plot_series(&samples);

        assert_eq!(series[0].len(), 5);
        assert_eq!(series[0][0], PlotPoint::new(0.0, 1.0));
        assert!(series[0][1].x.is_nan() && series[0][1].y.is_nan());
        assert_eq!(series[0][2], PlotPoint::new(1.0, 2.0));
        assert_eq!(series[0][3], PlotPoint::new(2.0, 3.0));
        assert!(series[0][4].x.is_nan() && series[0][4].y.is_nan());
        assert_eq!(series[1].len(), 3);
        assert_eq!(series[1][0], PlotPoint::new(0.0, 1.0));
        assert_eq!(series[1][1], PlotPoint::new(1.0, 2.0));
        assert!(series[1][2].x.is_nan() && series[1][2].y.is_nan());
        assert_eq!(
            series[2],
            vec![PlotPoint::new(3.0, 4.0), PlotPoint::new(4.0, 5.0)]
        );
    }
}
