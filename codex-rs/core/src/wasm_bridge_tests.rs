use super::WasmBridgeKernel;
use crate::wasm_extension::HOST_APP_SERVER_RPC;
use crate::wasm_extension::HOST_TCP_SOCKET;
use crate::wasm_extension::HOST_WEBSOCKET_REQUEST;
use pretty_assertions::assert_eq;
use serde_json::Value;

fn json(input: &str) -> Value {
    match serde_json::from_str(input) {
        Ok(value) => value,
        Err(err) => panic!("expected valid JSON in test: {err}"),
    }
}

#[test]
fn submit_requires_init() {
    let kernel = WasmBridgeKernel::default();

    let response = kernel.handle_submit_json(
        r#"{"correlation_id":"req-1","kind":"prompt","payload":{"text":"hello"}}"#,
    );

    assert_eq!(
        json(&response),
        json(
            r#"{"correlation_id":"req-1","error":{"code":"internal","message":"bridge not initialized","details":null}}"#
        )
    );
}

#[test]
fn cancel_clears_pending_and_records_cancelled() {
    let kernel = WasmBridgeKernel::default();
    kernel.handle_init_json(r#"{"runtime":"node"}"#);
    kernel.handle_submit_json(
        r#"{"correlation_id":"cx1","kind":"tool","payload":{"http":{"url":"https://example.com"}}}"#,
    );
    assert!(kernel.test_pending_requests_contains("cx1"));
    assert!(!kernel.test_cancelled_requests_contains("cx1"));

    kernel.handle_cancel_json(r#"{"correlation_id":"cx1","reason":"user"}"#);

    assert!(
        !kernel.test_pending_requests_contains("cx1"),
        "cancel should remove correlation_id from pending set"
    );
    assert!(kernel.test_cancelled_requests_contains("cx1"));
}

#[test]
fn submit_queues_host_call_from_payload() {
    let kernel = WasmBridgeKernel::default();
    let init_response = kernel.handle_init_json(r#"{"runtime":"node"}"#);
    let submit_response = kernel.handle_submit_json(
        r#"{"correlation_id":"req-2","kind":"prompt","payload":{"host_call":{"capability":"host_http_request","payload":{"url":"https://example.com"}}}}"#,
    );

    assert_eq!(json(&init_response), json(r#"{"ok":true}"#));
    assert_eq!(
        json(&submit_response),
        json(r#"{"correlation_id":"req-2","status":"accepted"}"#)
    );
    assert_eq!(
        kernel.take_pending_host_calls(),
        vec![super::WasmBridgeHostCall {
            correlation_id: "req-2".to_string(),
            capability: "host_http_request".to_string(),
            payload: Some(json(r#"{"url":"https://example.com"}"#)),
        }]
    );
}

#[test]
fn submit_cli_empty_args_returns_invalid_cli() {
    let kernel = WasmBridgeKernel::default();
    kernel.handle_init_json(r#"{"runtime":"node"}"#);

    let response = kernel.handle_submit_json(
        r#"{"correlation_id":"bad-cli","kind":"cli","payload":{"args":[],"cwd":"/"}}"#,
    );

    assert_eq!(
        json(&response),
        json(
            r#"{"correlation_id":"bad-cli","error":{"code":"invalid_cli","message":"cli.args must be non-empty","details":null}}"#
        )
    );
}

#[test]
fn submit_cli_non_string_arg_returns_invalid_cli() {
    let kernel = WasmBridgeKernel::default();
    kernel.handle_init_json(r#"{"runtime":"node"}"#);

    let response = kernel.handle_submit_json(
        r#"{"correlation_id":"bad-cli2","kind":"cli","payload":{"args":[1,"b"]}}"#,
    );

    assert_eq!(
        json(&response),
        json(
            r#"{"correlation_id":"bad-cli2","error":{"code":"invalid_cli","message":"cli.args elements must be JSON strings","details":null}}"#
        )
    );
}

#[test]
fn wasm_bridge_exec_host_event_roundtrips_json() {
    let chunk = super::WasmBridgeExecHostEvent::ExecChunk {
        correlation_id: "c1".to_string(),
        stream: "stdout".to_string(),
        data: "hello".to_string(),
    };
    let value = serde_json::to_value(&chunk).unwrap();
    let back: super::WasmBridgeExecHostEvent = serde_json::from_value(value).unwrap();
    assert_eq!(chunk, back);

    let exit = super::WasmBridgeExecHostEvent::ExecExit {
        correlation_id: "c1".to_string(),
        exit_code: 42,
        signal: None,
        cancelled: false,
    };
    let value = serde_json::to_value(&exit).unwrap();
    let back: super::WasmBridgeExecHostEvent = serde_json::from_value(value).unwrap();
    assert_eq!(exit, back);
}

#[test]
fn cancel_payload_roundtrips_json() {
    let cancel = super::WasmBridgeCancelRequest {
        correlation_id: "cx".to_string(),
        reason: Some("user".to_string()),
    };
    let s = serde_json::to_string(&cancel).unwrap();
    let back: super::WasmBridgeCancelRequest = serde_json::from_str(&s).unwrap();
    assert_eq!(cancel, back);
}

#[test]
fn exec_lifecycle_with_typed_host_callbacks() {
    let kernel = WasmBridgeKernel::default();
    kernel.handle_init_json("{}");

    let submit_response = kernel.handle_submit_json(
        r#"{"correlation_id":"e2e","kind":"exec","payload":{"exec":{"command":"sh","args":["-c","true"]}}}"#,
    );
    assert_eq!(
        json(&submit_response),
        json(r#"{"correlation_id":"e2e","status":"accepted"}"#)
    );

    let _calls = kernel.take_pending_host_calls();

    let chunk_payload = serde_json::to_value(super::WasmBridgeExecHostEvent::ExecChunk {
        correlation_id: "e2e".to_string(),
        stream: "stdout".to_string(),
        data: "mock".to_string(),
    })
    .unwrap();
    kernel.handle_deliver_callback_json(
        &serde_json::json!({
            "correlation_id": "e2e",
            "capability": "exec",
            "payload": chunk_payload,
        })
        .to_string(),
    );

    let exit_payload = serde_json::to_value(super::WasmBridgeExecHostEvent::ExecExit {
        correlation_id: "e2e".to_string(),
        exit_code: 0,
        signal: None,
        cancelled: false,
    })
    .unwrap();
    kernel.handle_deliver_callback_json(
        &serde_json::json!({
            "correlation_id": "e2e",
            "capability": "exec",
            "payload": exit_payload,
        })
        .to_string(),
    );

    assert_eq!(kernel.delivered_callback_count(), 2);
}

#[test]
fn submit_cli_kind_queues_exec_host_call_when_args_non_empty() {
    let kernel = WasmBridgeKernel::default();
    kernel.handle_init_json(r#"{"runtime":"node"}"#);

    let submit_response = kernel.handle_submit_json(
        r#"{"correlation_id":"req-cli","kind":"cli","payload":{"args":["echo","wasm-cli"],"cwd":"/tmp"}}"#,
    );

    assert_eq!(
        json(&submit_response),
        json(r#"{"correlation_id":"req-cli","status":"accepted"}"#)
    );
    assert_eq!(
        kernel.take_pending_host_calls(),
        vec![super::WasmBridgeHostCall {
            correlation_id: "req-cli".to_string(),
            capability: "host_exec_request".to_string(),
            payload: Some(json(
                r#"{"command":"echo","args":["wasm-cli"],"cwd":"/tmp","env":null,"timeout_ms":null,"stdin":null}"#,
            )),
        }]
    );
}

#[test]
fn submit_http_stream_flag_is_visible_on_host_payload() {
    let kernel = WasmBridgeKernel::default();
    kernel.handle_init_json(r#"{"runtime":"node"}"#);

    let submit_response = kernel.handle_submit_json(
        r#"{"correlation_id":"req-stream","kind":"tool","payload":{"http":{"url":"https://example.com/stream","method":"GET","stream_response":true}}}"#,
    );

    assert_eq!(
        json(&submit_response),
        json(r#"{"correlation_id":"req-stream","status":"accepted"}"#)
    );
    let calls = kernel.take_pending_host_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].capability, "host_http_request");
    let payload = calls[0].payload.as_ref().unwrap();
    assert_eq!(payload.get("stream_response"), Some(&Value::Bool(true)));
    assert_eq!(
        serde_json::from_value::<super::WasmBridgeHttpRequest>(payload.clone()).unwrap(),
        super::WasmBridgeHttpRequest {
            url: "https://example.com/stream".to_string(),
            method: Some("GET".to_string()),
            headers: None,
            body: None,
            timeout_ms: None,
            stream_response: Some(true),
        }
    );
}

#[test]
fn submit_structured_http_queues_host_call() {
    let kernel = WasmBridgeKernel::default();
    kernel.handle_init_json(r#"{"runtime":"node"}"#);

    let submit_response = kernel.handle_submit_json(
        r#"{"correlation_id":"req-http","kind":"tool","payload":{"http":{"url":"https://example.com","method":"GET"}}}"#,
    );

    assert_eq!(
        json(&submit_response),
        json(r#"{"correlation_id":"req-http","status":"accepted"}"#)
    );
    assert_eq!(
        kernel.take_pending_host_calls(),
        vec![super::WasmBridgeHostCall {
            correlation_id: "req-http".to_string(),
            capability: "host_http_request".to_string(),
            payload: Some(json(r#"{"url":"https://example.com","method":"GET"}"#)),
        }]
    );
}

#[test]
fn submit_structured_extension_keys_queue_expected_host_calls() {
    let kernel_row =
        |correlation_id: &str, capability: &str, payload: &str| super::WasmBridgeHostCall {
            correlation_id: correlation_id.to_string(),
            capability: capability.to_string(),
            payload: Some(json(payload)),
        };
    let kernel = WasmBridgeKernel::default();
    kernel.handle_init_json(r#"{"runtime":"node"}"#);

    let submit_response = kernel.handle_submit_json(
        r#"{"correlation_id":"req-ws","kind":"tool","payload":{"ws":{"url":"wss://example.com/w"}}}"#,
    );
    assert_eq!(
        json(&submit_response),
        json(r#"{"correlation_id":"req-ws","status":"accepted"}"#)
    );
    assert_eq!(
        kernel.take_pending_host_calls(),
        vec![kernel_row(
            "req-ws",
            HOST_WEBSOCKET_REQUEST,
            r#"{"url":"wss://example.com/w"}"#,
        )]
    );

    let submit_response = kernel.handle_submit_json(
        r#"{"correlation_id":"req-tcp","kind":"tool","payload":{"tcp":{"host":"127.0.0.1","port":9}}}"#,
    );
    assert_eq!(
        json(&submit_response),
        json(r#"{"correlation_id":"req-tcp","status":"accepted"}"#)
    );
    assert_eq!(
        kernel.take_pending_host_calls(),
        vec![kernel_row(
            "req-tcp",
            HOST_TCP_SOCKET,
            r#"{"host":"127.0.0.1","port":9}"#,
        )]
    );

    let submit_response = kernel.handle_submit_json(
        r#"{"correlation_id":"req-rpc","kind":"tool","payload":{"app_server_rpc":{"method":"thread/read","params":{}}}}"#,
    );
    assert_eq!(
        json(&submit_response),
        json(r#"{"correlation_id":"req-rpc","status":"accepted"}"#)
    );
    assert_eq!(
        kernel.take_pending_host_calls(),
        vec![kernel_row(
            "req-rpc",
            HOST_APP_SERVER_RPC,
            r#"{"method":"thread/read","params":{}}"#,
        )]
    );
}

#[test]
fn submit_queues_exec_host_call_from_payload() {
    let kernel = WasmBridgeKernel::default();
    kernel.handle_init_json(r#"{"runtime":"node"}"#);

    let submit_response = kernel.handle_submit_json(
        r#"{"correlation_id":"req-exec","kind":"exec","payload":{"exec":{"command":"bash","args":["-lc","echo hi"],"cwd":"/tmp/demo","env":{"TERM":"xterm-256color"},"timeout_ms":5000,"stdin":"echo from wasm\n"}}}"#,
    );

    assert_eq!(
        json(&submit_response),
        json(r#"{"correlation_id":"req-exec","status":"accepted"}"#)
    );
    assert_eq!(
        kernel.take_pending_host_calls(),
        vec![super::WasmBridgeHostCall {
            correlation_id: "req-exec".to_string(),
            capability: "host_exec_request".to_string(),
            payload: Some(json(
                r#"{"command":"bash","args":["-lc","echo hi"],"cwd":"/tmp/demo","env":{"TERM":"xterm-256color"},"timeout_ms":5000,"stdin":"echo from wasm\n"}"#,
            )),
        }]
    );
}

#[test]
fn callback_delivery_clears_pending_request() {
    let kernel = WasmBridgeKernel::default();
    kernel.handle_init_json(r#"{"runtime":"node"}"#);
    kernel.handle_submit_json(
        r#"{"correlation_id":"req-3","kind":"prompt","payload":{"text":"hello"}}"#,
    );

    kernel.handle_deliver_callback_json(
        r#"{"correlation_id":"req-3","capability":"host_exec_request","payload":{"exit_code":0}}"#,
    );
    kernel.handle_cancel_json(r#"{"correlation_id":"req-3","reason":"late"}"#);

    assert_eq!(kernel.delivered_callback_count(), 1);
    assert_eq!(kernel.take_pending_host_calls(), Vec::new());
}
