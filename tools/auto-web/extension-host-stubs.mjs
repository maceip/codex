/**
 * Synthetic wasm `host_*` handlers for extension transports (tests / browser harness).
 * Mirrors network callback shape so the bridge kernel can drain pending state.
 */

/**
 * @param {(json: string) => void} deliverJson typically `(j) => wasm.codex_deliver_callback(j)`
 */
export function installSyntheticExtensionHosts(deliverJson) {
  const netOk = (cid, body) =>
    deliverJson(
      JSON.stringify({
        correlation_id: cid,
        capability: "network",
        payload: {
          kind: "http_response",
          correlation_id: cid,
          status: 200,
          headers: {},
          body,
        },
      }),
    );

  globalThis.host_websocket_request = (json) => {
    const p = JSON.parse(json);
    netOk(p.correlation_id, '{"stub":"extension_ws"}');
  };

  globalThis.host_tcp_socket = (json) => {
    const p = JSON.parse(json);
    netOk(p.correlation_id, '{"stub":"extension_tcp"}');
  };

  globalThis.host_app_server_rpc = (json) => {
    const p = JSON.parse(json);
    netOk(p.correlation_id, '{"jsonrpc":"2.0","result":{},"id":null}');
  };
}
