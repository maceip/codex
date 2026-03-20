//! Extension capability identifiers and payload sketches for milestones beyond the
//! thin `codex-wasm-bridge`: embedding **`codex-core`** on `wasm32`, raw **TCP**,
//! **WebSocket**, and **app-server**-shaped RPC over the host boundary.
//!
//! - Hosts that do not implement a capability **MUST** answer with `missing_capability`
//!   (same rule as [`crate::wasm_bridge`] today).
//! - Types here are **contract sketches** for serde-stable JSON; structured `codex_submit`
//!   keys are wired through [`crate::wasm_bridge`] and wasm-bindgen imports — hosts may
//!   still answer `missing_capability`.
//!
//! Keep string constants in sync with `codex-wasm-bridge::extension_ids` (native tests
//! assert equality) and with `auto-web/abi/host-contract.json` → `extension_interfaces`.

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

// --- wasm import / host function names (planned) -----------------------------------------

/// WebSocket proxy: connect, send frames, receive async callbacks (planned).
pub const HOST_WEBSOCKET_REQUEST: &str = "host_websocket_request";

/// TCP socket: connect, read/write bytes, close (planned; browsers may omit).
pub const HOST_TCP_SOCKET: &str = "host_tcp_socket";

/// Forward an app-server v2-style JSON-RPC envelope to the host (IDE tab, worker, etc.).
pub const HOST_APP_SERVER_RPC: &str = "host_app_server_rpc";

// --- Structured `codex_submit` payload keys (mirror `http`, `exec`, …) -------------------

pub const SUBMIT_KEY_WEBSOCKET: &str = "ws";
pub const SUBMIT_KEY_TCP: &str = "tcp";
pub const SUBMIT_KEY_APP_SERVER_RPC: &str = "app_server_rpc";

// --- Payload sketches --------------------------------------------------------------------

/// Sketch: host opens (or reuses) a WebSocket; results arrive via `capability: network` or a
/// dedicated callback `kind` once stabilized.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WasmBridgeWebSocketHandshake {
    pub url: String,
    #[serde(default)]
    pub protocols: Vec<String>,
    #[serde(default)]
    pub headers: Option<Value>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// Sketch: one logical TCP connection identified by `correlation_id` across host calls.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WasmBridgeTcpConnect {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub tls: Option<bool>,
}

/// Sketch: forward app-server JSON-RPC (`thread/read`, …) without pulling `reqwest` into wasm.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WasmBridgeAppServerRpcEnvelope {
    /// e.g. `thread/read` (v2 resource/method shape).
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
    #[serde(default)]
    pub id: Option<Value>,
}

/// Maps [`SUBMIT_KEY_WEBSOCKET`] / [`SUBMIT_KEY_TCP`] / [`SUBMIT_KEY_APP_SERVER_RPC`] to import names.
pub fn structured_key_to_capability(key: &str) -> Option<&'static str> {
    match key {
        "ws" => Some(HOST_WEBSOCKET_REQUEST),
        "tcp" => Some(HOST_TCP_SOCKET),
        "app_server_rpc" => Some(HOST_APP_SERVER_RPC),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_key_routes_match_constants() {
        assert_eq!(
            structured_key_to_capability(SUBMIT_KEY_WEBSOCKET),
            Some(HOST_WEBSOCKET_REQUEST)
        );
        assert_eq!(
            structured_key_to_capability(SUBMIT_KEY_TCP),
            Some(HOST_TCP_SOCKET)
        );
        assert_eq!(
            structured_key_to_capability(SUBMIT_KEY_APP_SERVER_RPC),
            Some(HOST_APP_SERVER_RPC)
        );
    }
}
