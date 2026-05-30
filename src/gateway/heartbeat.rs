use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

/// Build a HEARTBEAT payload (op 1).
pub fn build_heartbeat(seq: u64) -> Value {
    json!({
        "op": 1,
        "d": if seq > 0 { Value::Number(seq.into()) } else { Value::Null }
    })
}

/// Generate a random jitter value in [0, interval_ms) using system time.
pub fn jitter_ms(interval_ms: u64) -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    (nanos as u64) % interval_ms
}
