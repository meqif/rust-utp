use std::time;

/// Return current time in microseconds since the UNIX epoch.
pub fn now_microseconds() -> Timestamp {
    let t = time::SystemTime::now()
        .duration_since(time::UNIX_EPOCH)
        .unwrap_or_else(|e| e.duration());
    (t.as_secs().wrapping_mul(1_000_000) as u32).wrapping_add(t.subsec_nanos() / 1000).into()
}
