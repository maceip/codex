use crate::extension_ids;
use codex_core::wasm_extension as core_ext;
use pretty_assertions::assert_eq;

#[test]
fn extension_capability_ids_match_codex_core() {
    assert_eq!(
        extension_ids::HOST_WEBSOCKET_REQUEST,
        core_ext::HOST_WEBSOCKET_REQUEST
    );
    assert_eq!(extension_ids::HOST_TCP_SOCKET, core_ext::HOST_TCP_SOCKET);
    assert_eq!(
        extension_ids::HOST_APP_SERVER_RPC,
        core_ext::HOST_APP_SERVER_RPC
    );
    assert_eq!(
        extension_ids::SUBMIT_KEY_WEBSOCKET,
        core_ext::SUBMIT_KEY_WEBSOCKET
    );
    assert_eq!(extension_ids::SUBMIT_KEY_TCP, core_ext::SUBMIT_KEY_TCP);
    assert_eq!(
        extension_ids::SUBMIT_KEY_APP_SERVER_RPC,
        core_ext::SUBMIT_KEY_APP_SERVER_RPC
    );
}
