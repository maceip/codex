#[cfg(target_arch = "wasm32")]
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
#[cfg(target_arch = "wasm32")]
use std::collections::BTreeSet;
#[cfg(target_arch = "wasm32")]
use std::sync::Mutex;
use std::sync::OnceLock;

#[cfg(not(target_arch = "wasm32"))]
use codex_core::wasm_bridge::WasmBridgeHostCall;
#[cfg(not(target_arch = "wasm32"))]
use codex_core::wasm_bridge::WasmBridgeKernel;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::wasm_bindgen;

pub mod extension_ids;

#[cfg(target_arch = "wasm32")]
#[derive(Default)]
struct BridgeState {
    initialized: bool,
    pending_requests: BTreeSet<String>,
    cancelled_requests: BTreeSet<String>,
    delivered_callbacks: usize,
}

#[cfg(target_arch = "wasm32")]
#[derive(Deserialize)]
struct SubmitRequest {
    correlation_id: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    payload: Option<Value>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Deserialize)]
struct ExecRequestPayload {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    arg0: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    env: Option<serde_json::Map<String, Value>>,
    #[serde(default)]
    timeout_ms: Option<u64>,
    #[serde(default)]
    stdin: Option<String>,
    #[serde(default)]
    transport: Option<ExecTransportPayload>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Deserialize)]
struct ExecTransportPayload {
    kind: String,
    #[serde(default)]
    stdin_policy: Option<String>,
    #[serde(default)]
    size: Option<ExecTerminalSizePayload>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Deserialize)]
struct ExecTerminalSizePayload {
    rows: u16,
    cols: u16,
}

#[cfg(target_arch = "wasm32")]
#[derive(Deserialize)]
struct CallbackEnvelope {
    #[serde(default)]
    correlation_id: Option<String>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Deserialize)]
struct CancelRequest {
    correlation_id: String,
    #[serde(default)]
    reason: Option<String>,
}

#[cfg(target_arch = "wasm32")]
struct BridgeHostCall {
    correlation_id: String,
    capability: String,
    payload: Option<Value>,
}

#[cfg(target_arch = "wasm32")]
static BRIDGE_STATE: OnceLock<Mutex<BridgeState>> = OnceLock::new();
#[cfg(not(target_arch = "wasm32"))]
static BRIDGE_KERNEL: OnceLock<WasmBridgeKernel> = OnceLock::new();

#[cfg(target_arch = "wasm32")]
#[derive(Serialize)]
struct OkResponse {
    ok: bool,
}

#[cfg(target_arch = "wasm32")]
#[derive(Serialize)]
struct AcceptedResponse<'a> {
    correlation_id: &'a str,
    status: &'static str,
}

#[derive(Serialize)]
struct ErrorResponse<'a> {
    correlation_id: &'a str,
    error: ErrorPayload<'a>,
}

#[derive(Serialize)]
struct ErrorPayload<'a> {
    code: &'a str,
    message: &'a str,
    details: Option<Value>,
}

#[cfg(target_arch = "wasm32")]
fn bridge_state() -> &'static Mutex<BridgeState> {
    BRIDGE_STATE.get_or_init(|| Mutex::new(BridgeState::default()))
}

#[cfg(not(target_arch = "wasm32"))]
fn bridge_kernel() -> &'static WasmBridgeKernel {
    BRIDGE_KERNEL.get_or_init(WasmBridgeKernel::default)
}

#[cfg(target_arch = "wasm32")]
fn lock_bridge_state() -> std::sync::MutexGuard<'static, BridgeState> {
    bridge_state()
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
    serialize_json(&ErrorResponse {
        correlation_id,
        error: ErrorPayload {
            code,
            message,
            details,
        },
    })
}

fn missing_capability_callback_json(correlation_id: &str, capability: &str) -> String {
    error_response(
        correlation_id,
        "missing_capability",
        "host capability is not available in this runtime",
        Some(serde_json::json!({
            "capability": capability,
        })),
    )
}

#[cfg(target_arch = "wasm32")]
fn parse_json<T: for<'de> Deserialize<'de>>(input: &str) -> Result<T, Value> {
    serde_json::from_str(input).map_err(|err| {
        serde_json::json!({
            "parse_error": err.to_string(),
        })
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn handle_init(config_json: &str) -> String {
    bridge_kernel().handle_init_json(config_json)
}

#[cfg(target_arch = "wasm32")]
fn handle_init(config_json: &str) -> String {
    if let Err(details) = parse_json::<Value>(config_json) {
        return error_response(
            "lifecycle",
            "internal",
            "invalid init config JSON",
            Some(details),
        );
    }

    let mut state = lock_bridge_state();
    state.initialized = true;
    state.pending_requests.clear();
    state.cancelled_requests.clear();
    state.delivered_callbacks = 0;
    serialize_json(&OkResponse { ok: true })
}

#[cfg(not(target_arch = "wasm32"))]
fn handle_submit(request_json: &str) -> String {
    let response = bridge_kernel().handle_submit_json(request_json);
    let pending = bridge_kernel().take_pending_host_calls();
    dispatch_pending_host_calls(pending);
    response
}

#[cfg(target_arch = "wasm32")]
fn handle_submit(request_json: &str) -> String {
    let request = match parse_json::<SubmitRequest>(request_json) {
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

    let mut state = lock_bridge_state();
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
                    return error_response(&request.correlation_id, "invalid_cli", message, None);
                }
            }
        }
        _ => host_call_from_request(&request),
    };

    state
        .pending_requests
        .insert(request.correlation_id.clone());
    drop(state);

    if let Some(host_call) = host_call {
        dispatch_host_call(host_call);
    }

    serialize_json(&AcceptedResponse {
        correlation_id: &request.correlation_id,
        status: "accepted",
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn handle_deliver_callback(result_json: &str) {
    bridge_kernel().handle_deliver_callback_json(result_json);
}

#[cfg(target_arch = "wasm32")]
fn handle_deliver_callback(result_json: &str) {
    let callback = match parse_json::<CallbackEnvelope>(result_json) {
        Ok(callback) => callback,
        Err(_) => return,
    };

    let mut state = lock_bridge_state();
    state.delivered_callbacks += 1;
    if let Some(correlation_id) = callback.correlation_id {
        state.pending_requests.remove(&correlation_id);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn handle_cancel(cancel_json: &str) {
    bridge_kernel().handle_cancel_json(cancel_json);
}

#[cfg(target_arch = "wasm32")]
fn handle_cancel(cancel_json: &str) {
    let cancel = match parse_json::<CancelRequest>(cancel_json) {
        Ok(cancel) => cancel,
        Err(_) => return,
    };

    let mut state = lock_bridge_state();
    let _ = cancel.reason;
    state.pending_requests.remove(&cancel.correlation_id);
    state.cancelled_requests.insert(cancel.correlation_id);
}

#[cfg(not(target_arch = "wasm32"))]
fn handle_shutdown() {
    bridge_kernel().handle_shutdown();
}

#[cfg(target_arch = "wasm32")]
fn handle_shutdown() {
    let mut state = lock_bridge_state();
    *state = BridgeState::default();
}

#[cfg(all(not(target_arch = "wasm32"), not(test)))]
fn dispatch_pending_host_calls(host_calls: Vec<WasmBridgeHostCall>) {
    for host_call in host_calls {
        bridge_kernel().handle_deliver_callback_json(&missing_capability_callback_json(
            &host_call.correlation_id,
            &host_call.capability,
        ));
    }
}

#[cfg(all(not(target_arch = "wasm32"), test))]
fn dispatch_pending_host_calls(host_calls: Vec<WasmBridgeHostCall>) {
    let kernel = bridge_kernel();
    for host_call in host_calls {
        if host_call.capability == "host_exec_request" {
            let cid = &host_call.correlation_id;
            let chunk = serde_json::json!({
                "correlation_id": cid,
                "capability": "exec",
                "payload": {
                    "kind": "exec_chunk",
                    "correlation_id": cid,
                    "stream": "stdout",
                    "data": "mock-out"
                }
            });
            kernel.handle_deliver_callback_json(&chunk.to_string());
            let exit = serde_json::json!({
                "correlation_id": cid,
                "capability": "exec",
                "payload": {
                    "kind": "exec_exit",
                    "correlation_id": cid,
                    "exit_code": 0,
                    "signal": null,
                    "cancelled": false
                }
            });
            kernel.handle_deliver_callback_json(&exit.to_string());
        } else if matches!(
            host_call.capability.as_str(),
            "host_http_request"
                | "host_websocket_request"
                | "host_tcp_socket"
                | "host_app_server_rpc"
        ) {
            let cid = host_call.correlation_id.as_str();
            let body = match host_call.capability.as_str() {
                "host_http_request" => r#"{"mock":"http"}"#,
                "host_websocket_request" => r#"{"mock":"ws"}"#,
                "host_tcp_socket" => r#"{"mock":"tcp"}"#,
                _ => r#"{"mock":"rpc"}"#,
            };
            let cb = serde_json::json!({
                "correlation_id": cid,
                "capability": "network",
                "payload": {
                    "kind": "http_response",
                    "correlation_id": cid,
                    "status": 200,
                    "headers": serde_json::json!({}),
                    "body": body,
                }
            });
            kernel.handle_deliver_callback_json(&cb.to_string());
        } else {
            kernel.handle_deliver_callback_json(&missing_capability_callback_json(
                &host_call.correlation_id,
                &host_call.capability,
            ));
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn host_call_from_request(request: &SubmitRequest) -> Option<BridgeHostCall> {
    let payload = request.payload.as_ref()?;
    if let Some(host_call) = payload.get("host_call") {
        return Some(BridgeHostCall {
            correlation_id: request.correlation_id.clone(),
            capability: host_call.get("capability")?.as_str()?.to_string(),
            payload: host_call.get("payload").cloned(),
        });
    }

    if let Some(exec_val) = payload.get("exec") {
        if let Ok(exec) = serde_json::from_value::<ExecRequestPayload>(exec_val.clone()) {
            return Some(BridgeHostCall {
                correlation_id: request.correlation_id.clone(),
                capability: "host_exec_request".to_string(),
                payload: Some(exec_request_payload_json(exec)),
            });
        }
    }

    const STRUCTURED: &[(&str, &str)] = &[
        ("http", "host_http_request"),
        (
            crate::extension_ids::SUBMIT_KEY_WEBSOCKET,
            crate::extension_ids::HOST_WEBSOCKET_REQUEST,
        ),
        (
            crate::extension_ids::SUBMIT_KEY_TCP,
            crate::extension_ids::HOST_TCP_SOCKET,
        ),
        (
            crate::extension_ids::SUBMIT_KEY_APP_SERVER_RPC,
            crate::extension_ids::HOST_APP_SERVER_RPC,
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
            return Some(BridgeHostCall {
                correlation_id: request.correlation_id.clone(),
                capability: (*capability).to_string(),
                payload: Some(value.clone()),
            });
        }
    }

    None
}

#[cfg(target_arch = "wasm32")]
fn try_host_call_from_cli_payload(
    correlation_id: &str,
    payload: &Value,
) -> Result<BridgeHostCall, &'static str> {
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
    let exec = ExecRequestPayload {
        command,
        args: rest,
        arg0: None,
        cwd,
        env: None,
        timeout_ms: None,
        stdin: None,
        transport: None,
    };
    Ok(BridgeHostCall {
        correlation_id: correlation_id.to_string(),
        capability: "host_exec_request".to_string(),
        payload: Some(exec_request_payload_json(exec)),
    })
}

#[cfg(target_arch = "wasm32")]
fn dispatch_host_call(host_call: BridgeHostCall) {
    let request_json = host_call_request_json(&host_call);
    match host_call.capability.as_str() {
        "host_exec_request" => host_imports::host_exec_request(&request_json),
        "host_http_request" => host_imports::host_http_request(&request_json),
        "host_fs_read" => host_imports::host_fs_read(&request_json),
        "host_fs_write" => host_imports::host_fs_write(&request_json),
        "host_fs_list" => host_imports::host_fs_list(&request_json),
        "host_fs_stat" => host_imports::host_fs_stat(&request_json),
        "host_fs_remove" => host_imports::host_fs_remove(&request_json),
        "host_secret_get" => host_imports::host_secret_get(&request_json),
        "host_secret_set" => host_imports::host_secret_set(&request_json),
        "host_sandbox_apply" => host_imports::host_sandbox_apply(&request_json),
        "host_websocket_request" => host_imports::host_websocket_request(&request_json),
        "host_tcp_socket" => host_imports::host_tcp_socket(&request_json),
        "host_app_server_rpc" => host_imports::host_app_server_rpc(&request_json),
        _ => handle_deliver_callback(&missing_capability_callback_json(
            &host_call.correlation_id,
            &host_call.capability,
        )),
    }
}

#[cfg(target_arch = "wasm32")]
fn host_call_request_json(host_call: &BridgeHostCall) -> String {
    let mut request = match host_call.payload.clone() {
        Some(Value::Object(map)) => map,
        Some(payload) => {
            let mut map = serde_json::Map::new();
            map.insert("payload".to_string(), payload);
            map
        }
        None => serde_json::Map::new(),
    };
    request.insert(
        "correlation_id".to_string(),
        Value::String(host_call.correlation_id.clone()),
    );
    serialize_json(&Value::Object(request))
}

#[cfg(target_arch = "wasm32")]
fn exec_request_payload_json(exec: ExecRequestPayload) -> Value {
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
        exec.env.map_or(Value::Null, Value::Object),
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
            exec_transport_payload_json(transport),
        );
    }
    Value::Object(payload)
}

#[cfg(target_arch = "wasm32")]
fn exec_transport_payload_json(transport: ExecTransportPayload) -> Value {
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

#[cfg(target_arch = "wasm32")]
#[allow(dead_code)]
mod host_imports {
    use wasm_bindgen::prelude::wasm_bindgen;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_name = host_exec_request)]
        pub fn host_exec_request(request_json: &str);

        #[wasm_bindgen(js_name = host_http_request)]
        pub fn host_http_request(request_json: &str);

        #[wasm_bindgen(js_name = host_fs_read)]
        pub fn host_fs_read(request_json: &str);

        #[wasm_bindgen(js_name = host_fs_write)]
        pub fn host_fs_write(request_json: &str);

        #[wasm_bindgen(js_name = host_fs_list)]
        pub fn host_fs_list(request_json: &str);

        #[wasm_bindgen(js_name = host_fs_stat)]
        pub fn host_fs_stat(request_json: &str);

        #[wasm_bindgen(js_name = host_fs_remove)]
        pub fn host_fs_remove(request_json: &str);

        #[wasm_bindgen(js_name = host_secret_get)]
        pub fn host_secret_get(request_json: &str);

        #[wasm_bindgen(js_name = host_secret_set)]
        pub fn host_secret_set(request_json: &str);

        #[wasm_bindgen(js_name = host_sandbox_apply)]
        pub fn host_sandbox_apply(request_json: &str);

        #[wasm_bindgen(js_name = host_websocket_request)]
        pub fn host_websocket_request(request_json: &str);

        #[wasm_bindgen(js_name = host_tcp_socket)]
        pub fn host_tcp_socket(request_json: &str);

        #[wasm_bindgen(js_name = host_app_server_rpc)]
        pub fn host_app_server_rpc(request_json: &str);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub async fn codex_init(config_json: String) -> String {
    handle_init(&config_json)
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub async fn codex_submit(request_json: String) -> String {
    handle_submit(&request_json)
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn codex_deliver_callback(result_json: String) {
    handle_deliver_callback(&result_json);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn codex_cancel(cancel_json: String) {
    handle_cancel(&cancel_json);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub async fn codex_shutdown() {
    handle_shutdown();
}

#[cfg(all(not(target_arch = "wasm32"), test))]
pub(crate) fn delivered_callback_count_for_tests() -> usize {
    bridge_kernel().delivered_callback_count()
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod wasm_bridge_contract_tests;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod extension_ids_sync_tests;
