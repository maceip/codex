//! Capability/import names for **planned** host surfaces. Must match
//! `codex_core::wasm_extension` (see `extension_ids_sync_tests`).
//!
//! Wasm-bindgen `extern "C"` blocks will grow here when TCP/WebSocket/app-server RPC ship.

/// Mirrors [`codex_core::wasm_extension::HOST_WEBSOCKET_REQUEST`].
pub const HOST_WEBSOCKET_REQUEST: &str = "host_websocket_request";

/// Mirrors [`codex_core::wasm_extension::HOST_TCP_SOCKET`].
pub const HOST_TCP_SOCKET: &str = "host_tcp_socket";

/// Mirrors [`codex_core::wasm_extension::HOST_APP_SERVER_RPC`].
pub const HOST_APP_SERVER_RPC: &str = "host_app_server_rpc";

pub const SUBMIT_KEY_WEBSOCKET: &str = "ws";
pub const SUBMIT_KEY_TCP: &str = "tcp";
pub const SUBMIT_KEY_APP_SERVER_RPC: &str = "app_server_rpc";
