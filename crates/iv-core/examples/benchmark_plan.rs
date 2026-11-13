//! Standalone planning benchmark: no device connection or output file creation.
#![forbid(unsafe_code)]

use iv_core::{Pattern, RunConfig, plan};

fn main() {
    let config = RunConfig {
        pattern: Pattern::Linear,
        step: 0.0000002,
        repeat: 99,
        ..RunConfig::default()
    };
    let begin = std::time::Instant::now();
    let planned = plan(&config).expect("valid resolution-aligned benchmark plan");
    let planning_s = begin.elapsed().as_secs_f64();
    assert_eq!(planned.points.len(), 10001);
    assert_eq!(planned.points[0].target, -0.001);
    assert_eq!(planned.points.last().unwrap().target, 0.001);
    for (index, point) in planned.points.iter().enumerate() {
        assert_eq!(point.point_index, index + 1);
        assert_eq!(point.total_points, 10001);
        assert_eq!(point.repeat_index, 1);
    }
    println!(
        "{}",
        serde_json::json!({
            "planning_s": planning_s, "points": planned.points.len(),
            "readings_per_point": 99, "meter_conversions": planned.points.len() * 99,
            "first": planned.points.first(), "last": planned.points.last(),
            "forward": planned.points.iter().filter(|point| point.direction == "forward").count(),
            "backward": planned.points.iter().filter(|point| point.direction == "backward").count(),
            "source_range": planned.source_range, "hardware_compliance": planned.hardware_compliance,
            "config": config,
        })
    );
}
