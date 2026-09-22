use std::time::Duration;

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    let mut out = String::new();
    if h > 0 {
        out.push_str(&format!("{h}h"));
    }
    if m > 0 || h > 0 {
        out.push_str(&format!("{m}min"));
    }
    out.push_str(&format!("{s}s"));
    out
}

pub fn format_status(msgs: u64, elapsed: Duration) -> String {
    let secs = elapsed.as_secs_f64();
    let rate = if secs > 0.0 {
        (msgs as f64 / secs).round() as u64
    } else {
        0
    };
    format!(
        "Messages: {msgs}, Time: {}, Rate: {rate}msg/s",
        format_duration(elapsed)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_under_a_minute() {
        assert_eq!(format_duration(Duration::from_secs(45)), "45s");
    }

    #[test]
    fn duration_minutes_and_seconds() {
        assert_eq!(format_duration(Duration::from_secs(90)), "1min30s");
    }

    #[test]
    fn duration_hours() {
        assert_eq!(format_duration(Duration::from_secs(3600)), "1h0min0s");
    }
}
