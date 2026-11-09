//! NEP-297 events for indexers: `EVENT_JSON:{"standard":"franchise",...}`.
use near_sdk::{env, serde_json};

pub fn emit(event: &str, data: serde_json::Value) {
    let log = serde_json::json!({
        "standard": "franchise",
        "version": "1.0.0",
        "event": event,
        "data": [data],
    });
    env::log_str(&format!("EVENT_JSON:{log}"));
}
