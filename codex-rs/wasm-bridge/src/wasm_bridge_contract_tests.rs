use crate::codex_cancel;
use crate::codex_deliver_callback;
use crate::codex_init;
use crate::codex_shutdown;
use crate::codex_submit;
use crate::delivered_callback_count_for_tests;
use futures::executor::block_on;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serial_test::serial;

fn json(input: &str) -> Value {
    match serde_json::from_str(input) {
        Ok(value) => value,
        Err(err) => panic!("test responses must be valid JSON: {err}"),
    }
}

#[test]
#[serial]
fn submit_requires_init() {
    block_on(codex_shutdown());

    let response = block_on(codex_submit(
        r#"{"correlation_id":"req-1","kind":"prompt","payload":{"text":"hello"}}"#.to_string(),
    ));

    assert_eq!(
        json(&response),
        json(
            r#"{"correlation_id":"req-1","error":{"code":"internal","message":"bridge not initialized","details":null}}"#
        )
    );
}

#[test]
#[serial]
fn init_and_submit_return_expected_json_shapes() {
    block_on(codex_shutdown());
    let init_response = block_on(codex_init(r#"{"runtime":"node"}"#.to_string()));
    let submit_response = block_on(codex_submit(
        r#"{"correlation_id":"req-2","kind":"prompt","payload":{"text":"hello"}}"#.to_string(),
    ));

    assert_eq!(json(&init_response), json(r#"{"ok":true}"#));
    assert_eq!(
        json(&submit_response),
        json(r#"{"correlation_id":"req-2","status":"accepted"}"#)
    );
}

#[test]
#[serial]
fn exec_payloads_are_accepted_through_host_call_seam() {
    block_on(codex_shutdown());
    block_on(codex_init(r#"{"runtime":"node"}"#.to_string()));
    assert_eq!(delivered_callback_count_for_tests(), 0);

    let submit_response = block_on(codex_submit(
        r#"{"correlation_id":"req-exec","kind":"exec","payload":{"exec":{"command":"bash","args":["-lc","echo hi"],"cwd":"/tmp/demo","env":{"TERM":"xterm-256color"},"timeout_ms":5000,"stdin":"echo from wasm\n"}}}"#.to_string(),
    ));

    assert_eq!(
        json(&submit_response),
        json(r#"{"correlation_id":"req-exec","status":"accepted"}"#)
    );
    assert_eq!(
        delivered_callback_count_for_tests(),
        2,
        "test mock host must deliver exec_chunk + exec_exit"
    );
}

#[test]
#[serial]
fn cli_empty_argv_is_rejected_with_invalid_cli() {
    block_on(codex_shutdown());
    block_on(codex_init(r#"{"runtime":"node"}"#.to_string()));

    let response = block_on(codex_submit(
        r#"{"correlation_id":"bad-cli","kind":"cli","payload":{"args":[],"cwd":"/"}}"#.to_string(),
    ));

    assert_eq!(
        json(&response),
        json(
            r#"{"correlation_id":"bad-cli","error":{"code":"invalid_cli","message":"cli.args must be non-empty","details":null}}"#
        )
    );
}

#[test]
#[serial]
fn http_stream_submit_is_accepted_for_phase3_seam() {
    block_on(codex_shutdown());
    block_on(codex_init(r#"{"runtime":"node"}"#.to_string()));

    let submit_response = block_on(codex_submit(
        r#"{"correlation_id":"req-hstream","kind":"tool","payload":{"http":{"url":"https://example.com/s","method":"GET","stream_response":true}}}"#
            .to_string(),
    ));

    assert_eq!(
        json(&submit_response),
        json(r#"{"correlation_id":"req-hstream","status":"accepted"}"#)
    );
}

#[test]
#[serial]
fn cli_kind_with_argv_maps_to_exec_host_call_seam() {
    block_on(codex_shutdown());
    block_on(codex_init(r#"{"runtime":"node"}"#.to_string()));

    let submit_response = block_on(codex_submit(
        r#"{"correlation_id":"req-cli","kind":"cli","payload":{"args":["echo","from-cli"],"cwd":"/tmp"}}"#
            .to_string(),
    ));

    assert_eq!(
        json(&submit_response),
        json(r#"{"correlation_id":"req-cli","status":"accepted"}"#)
    );
    assert_eq!(
        delivered_callback_count_for_tests(),
        2,
        "cli argv maps to host_exec_request; mock host completes exec"
    );
}

#[test]
#[serial]
fn callback_and_cancel_keep_lifecycle_calls_valid() {
    block_on(codex_shutdown());
    block_on(codex_init(r#"{"runtime":"node"}"#.to_string()));
    let accepted = block_on(codex_submit(
        r#"{"correlation_id":"req-3","kind":"prompt","payload":{"text":"hello"}}"#.to_string(),
    ));

    codex_deliver_callback(
        r#"{"correlation_id":"req-3","capability":"host_http_request","payload":{"status":200}}"#
            .to_string(),
    );
    codex_cancel(r#"{"correlation_id":"req-3","reason":"test"}"#.to_string());
    block_on(codex_shutdown());

    assert_eq!(
        json(&accepted),
        json(r#"{"correlation_id":"req-3","status":"accepted"}"#)
    );
    assert_eq!(
        json(&block_on(codex_submit(
            r#"{"correlation_id":"req-4","kind":"prompt","payload":{"text":"hello"}}"#.to_string(),
        ))),
        json(
            r#"{"correlation_id":"req-4","error":{"code":"internal","message":"bridge not initialized","details":null}}"#
        )
    );
}
