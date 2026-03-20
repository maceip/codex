use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Mutex;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct WasmBridgeSubmitRequest {
    pub correlation_id: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub payload: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct WasmBridgeCallbackEnvelope {
    #[serde(default)]
    pub correlation_id: Option<String>,
    #[serde(default)]
    pub capability: Option<String>,
    #[serde(default)]
    pub payload: Option<Value>,
    #[serde(default)]
    pub error: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct WasmBridgeCancelRequest {
    pub correlation_id: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct WasmBridgeHostCall {
    pub correlation_id: String,
    pub capability: String,
    #[serde(default)]
    pub payload: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct WasmBridgeExecRequest {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub arg0: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub stdin: Option<String>,
    #[serde(default)]
    pub transport: Option<WasmBridgeExecTransport>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct WasmBridgeExecTransport {
    pub kind: String,
    #[serde(default)]
    pub stdin_policy: Option<String>,
    #[serde(default)]
    pub size: Option<WasmBridgeExecTerminalSize>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct WasmBridgeExecTerminalSize {
    pub rows: u16,
    pub cols: u16,
}

/// Host HTTP request routed through `host_http_request` (Phase 3 bridge contract).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct WasmBridgeHttpRequest {
    pub url: String,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub headers: Option<Value>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// When true, the host delivers [`WasmBridgeHttpStreamChunk`] callbacks instead of buffering
    /// the full body into a single [`WasmBridgeHttpResponse`].
    #[serde(default)]
    pub stream_response: Option<bool>,
}

/// Buffered HTTP response (non-streaming path).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct WasmBridgeHttpResponse {
    pub correlation_id: String,
    pub status: u16,
    #[serde(default)]
    pub headers: Value,
    pub body: String,
}

/// One chunk from a streamed HTTP response (SSE, chunked transfer, or fetch body stream).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct WasmBridgeHttpStreamChunk {
    pub correlation_id: String,
    pub chunk: String,
    pub done: bool,
}

/// Inner `payload` for `capability: "exec"` callbacks (`host_call:exec` contract).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WasmBridgeExecHostEvent {
    ExecChunk {
        correlation_id: String,
        stream: String,
        data: String,
    },
    ExecExit {
        correlation_id: String,
        exit_code: i32,
        #[serde(default)]
        signal: Option<Value>,
        #[serde(default)]
        cancelled: bool,
    },
}

#[derive(Debug, Serialize)]
struct WasmBridgeOkResponse {
    ok: bool,
}

#[derive(Debug, Serialize)]
struct WasmBridgeAcceptedResponse<'a> {
    correlation_id: &'a str,
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct WasmBridgeErrorResponse<'a> {
    correlation_id: &'a str,
    error: WasmBridgeErrorPayload<'a>,
}

#[derive(Debug, Serialize)]
struct WasmBridgeErrorPayload<'a> {
    code: &'a str,
    message: &'a str,
    details: Option<Value>,
}

#[derive(Default)]
struct WasmBridgeState {
    initialized: bool,
    pending_requests: BTreeSet<String>,
    cancelled_requests: BTreeSet<String>,
    pending_host_calls: Vec<WasmBridgeHostCall>,
    delivered_callbacks: usize,
}

#[derive(Default)]
pub struct WasmBridgeKernel {
    state: Mutex<WasmBridgeState>,
}

impl WasmBridgeKernel {
    pub fn handle_init_json(&self, config_json: &str) -> String {
        if let Err(details) = parse_json::<Value>(config_json) {
            return error_response(
                "lifecycle",
                "internal",
                "invalid init config JSON",
                Some(details),
            );
        }

        let mut state = lock_state(&self.state);
        state.initialized = true;
        state.pending_requests.clear();
        state.cancelled_requests.clear();
        state.pending_host_calls.clear();
        state.delivered_callbacks = 0;
        serialize_json(&WasmBridgeOkResponse { ok: true })
    }

    pub fn handle_submit_json(&self, request_json: &str) -> String {
        let request = match parse_json::<WasmBridgeSubmitRequest>(request_json) {
            Ok(request) => request,
            Err(details) => {
                return error_response(
                    "unknown",
                    "internal",
                    "invalid submit request JSON",
                    Some(details),
                );
            }
        };

        let mut state = lock_state(&self.state);
        if !state.initialized {
            return error_response(
                &request.correlation_id,
                "internal",
                "bridge not initialized",
                None,
            );
        }

        let host_call = match request.kind.as_deref() {
            Some("cli") => {
                let Some(payload) = request.payload.as_ref() else {
                    return error_response(
                        &request.correlation_id,
                        "invalid_cli",
                        "cli submit requires a JSON payload object",
                        None,
                    );
                };
                match try_host_call_from_cli_payload(&request.correlation_id, payload) {
                    Ok(call) => Some(call),
                    Err(message) => {
                        return error_response(
                            &request.correlation_id,
                            "invalid_cli",
                            message,
                            None,
                        );
                    }
                }
            }
            _ => host_call_from_request(&request),
        };

        if let Some(host_call) = host_call {
            state.pending_host_calls.push(host_call);
        }
        state
            .pending_requests
            .insert(request.correlation_id.clone());

        serialize_json(&WasmBridgeAcceptedResponse {
            correlation_id: &request.correlation_id,
            status: "accepted",
        })
    }

    pub fn handle_deliver_callback_json(&self, result_json: &str) {
        let callback = match parse_json::<WasmBridgeCallbackEnvelope>(result_json) {
            Ok(callback) => callback,
            Err(_) => return,
        };

        let mut state = lock_state(&self.state);
        state.delivered_callbacks += 1;
        if let Some(correlation_id) = callback.correlation_id {
            state.pending_requests.remove(&correlation_id);
        }
    }

    pub fn handle_cancel_json(&self, cancel_json: &str) {
        let cancel = match parse_json::<WasmBridgeCancelRequest>(cancel_json) {
            Ok(cancel) => cancel,
            Err(_) => return,
        };

        let mut state = lock_state(&self.state);
        state.pending_requests.remove(&cancel.correlation_id);
        state.cancelled_requests.insert(cancel.correlation_id);
    }

    pub fn handle_shutdown(&self) {
        let mut state = lock_state(&self.state);
        *state = WasmBridgeState::default();
    }

    pub fn take_pending_host_calls(&self) -> Vec<WasmBridgeHostCall> {
        let mut state = lock_state(&self.state);
        std::mem::take(&mut state.pending_host_calls)
    }

    pub fn delivered_callback_count(&self) -> usize {
        let state = lock_state(&self.state);
        state.delivered_callbacks
    }

    #[cfg(test)]
    pub(crate) fn test_pending_requests_contains(&self, id: &str) -> bool {
        let state = lock_state(&self.state);
        state.pending_requests.contains(id)
    }

    #[cfg(test)]
    pub(crate) fn test_cancelled_requests_contains(&self, id: &str) -> bool {
        let state = lock_state(&self.state);
        state.cancelled_requests.contains(id)
    }
}

/// Maps `payload.*` shortcuts to host capabilities, including extension transports
/// (`ws`, `tcp`, `app_server_rpc`) from [`crate::wasm_extension`].
fn host_call_from_request(request: &WasmBridgeSubmitRequest) -> Option<WasmBridgeHostCall> {
    let payload = request.payload.as_ref()?;
    if let Some(host_call) = payload.get("host_call") {
        return Some(WasmBridgeHostCall {
            correlation_id: request.correlation_id.clone(),
            capability: host_call.get("capability")?.as_str()?.to_string(),
            payload: host_call.get("payload").cloned(),
        });
    }

    if let Some(exec_value) = payload.get("exec")
        && let Ok(exec) = serde_json::from_value::<WasmBridgeExecRequest>(exec_value.clone())
    {
        return Some(WasmBridgeHostCall {
            correlation_id: request.correlation_id.clone(),
            capability: "host_exec_request".to_string(),
            payload: Some(serialize_exec_request_payload(exec)),
        });
    }

    const STRUCTURED: &[(&str, &str)] = &[
        ("http", "host_http_request"),
        (
            crate::wasm_extension::SUBMIT_KEY_WEBSOCKET,
            crate::wasm_extension::HOST_WEBSOCKET_REQUEST,
        ),
        (
            crate::wasm_extension::SUBMIT_KEY_TCP,
            crate::wasm_extension::HOST_TCP_SOCKET,
        ),
        (
            crate::wasm_extension::SUBMIT_KEY_APP_SERVER_RPC,
            crate::wasm_extension::HOST_APP_SERVER_RPC,
        ),
        ("fs_read", "host_fs_read"),
        ("fs_write", "host_fs_write"),
        ("fs_list", "host_fs_list"),
        ("fs_stat", "host_fs_stat"),
        ("fs_remove", "host_fs_remove"),
        ("secret_get", "host_secret_get"),
        ("secret_set", "host_secret_set"),
        ("sandbox_apply", "host_sandbox_apply"),
    ];
    for (key, capability) in STRUCTURED {
        if let Some(value) = payload.get(*key) {
            return Some(WasmBridgeHostCall {
                correlation_id: request.correlation_id.clone(),
                capability: (*capability).to_string(),
                payload: Some(value.clone()),
            });
        }
    }

    None
}

/// `codex-cli` wasm mode submits `kind: "cli"` with `payload: { args, cwd? }`. Map argv to a single
/// `host_exec_request` (program = args[0], remainder = args[1..]). All argv elements must be JSON strings.
fn try_host_call_from_cli_payload(
    correlation_id: &str,
    payload: &Value,
) -> Result<WasmBridgeHostCall, &'static str> {
    let args = payload
        .get("args")
        .ok_or("cli payload requires an args array")?;
    let args_arr = args.as_array().ok_or("cli.args must be a JSON array")?;
    if args_arr.is_empty() {
        return Err("cli.args must be non-empty");
    }
    let mut strings = Vec::with_capacity(args_arr.len());
    for value in args_arr {
        let s = value
            .as_str()
            .ok_or("cli.args elements must be JSON strings")?;
        strings.push(s.to_string());
    }
    let command = strings[0].clone();
    let rest = strings[1..].to_vec();
    let cwd = payload
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(ToString::to_string);
    let exec = WasmBridgeExecRequest {
        command,
        args: rest,
        arg0: None,
        cwd,
        env: None,
        timeout_ms: None,
        stdin: None,
        transport: None,
    };
    Ok(WasmBridgeHostCall {
        correlation_id: correlation_id.to_string(),
        capability: "host_exec_request".to_string(),
        payload: Some(serialize_exec_request_payload(exec)),
    })
}

fn serialize_exec_request_payload(exec: WasmBridgeExecRequest) -> Value {
    let mut payload = serde_json::Map::new();
    payload.insert("command".to_string(), Value::String(exec.command));
    payload.insert(
        "args".to_string(),
        Value::Array(exec.args.into_iter().map(Value::String).collect()),
    );
    if let Some(arg0) = exec.arg0 {
        payload.insert("arg0".to_string(), Value::String(arg0));
    }
    payload.insert(
        "cwd".to_string(),
        exec.cwd.map_or(Value::Null, Value::String),
    );
    payload.insert(
        "env".to_string(),
        exec.env.map_or(Value::Null, |env| {
            Value::Object(
                env.into_iter()
                    .map(|(key, value)| (key, Value::String(value)))
                    .collect(),
            )
        }),
    );
    payload.insert(
        "timeout_ms".to_string(),
        exec.timeout_ms.map_or(Value::Null, |timeout_ms| {
            serde_json::Number::from(timeout_ms).into()
        }),
    );
    payload.insert(
        "stdin".to_string(),
        exec.stdin.map_or(Value::Null, Value::String),
    );
    if let Some(transport) = exec.transport {
        payload.insert(
            "transport".to_string(),
            serialize_exec_transport_payload(transport),
        );
    }
    Value::Object(payload)
}

fn serialize_exec_transport_payload(transport: WasmBridgeExecTransport) -> Value {
    let mut payload = serde_json::Map::new();
    payload.insert("kind".to_string(), Value::String(transport.kind));
    if let Some(stdin_policy) = transport.stdin_policy {
        payload.insert("stdin_policy".to_string(), Value::String(stdin_policy));
    }
    if let Some(size) = transport.size {
        payload.insert(
            "size".to_string(),
            serde_json::json!({
                "rows": size.rows,
                "cols": size.cols,
            }),
        );
    }
    Value::Object(payload)
}

fn lock_state(state: &Mutex<WasmBridgeState>) -> std::sync::MutexGuard<'_, WasmBridgeState> {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn serialize_json<T: Serialize>(value: &T) -> String {
    match serde_json::to_string(value) {
        Ok(json) => json,
        Err(_) => {
            "{\"correlation_id\":\"lifecycle\",\"error\":{\"code\":\"internal\",\"message\":\"failed to serialize bridge response\",\"details\":null}}".to_string()
        }
    }
}

fn error_response(
    correlation_id: &str,
    code: &'static str,
    message: &'static str,
    details: Option<Value>,
) -> String {
    serialize_json(&WasmBridgeErrorResponse {
        correlation_id,
        error: WasmBridgeErrorPayload {
            code,
            message,
            details,
        },
    })
}

fn parse_json<T: for<'de> Deserialize<'de>>(input: &str) -> Result<T, Value> {
    serde_json::from_str(input).map_err(|err| {
        serde_json::json!({
            "parse_error": err.to_string(),
        })
    })
}

#[cfg(test)]
#[path = "wasm_bridge_tests.rs"]
mod wasm_bridge_tests;
