/**
 * Extension transport imports (WebSocket / TCP / app-server RPC) for wasm-bindgen.
 * Production Node loader ships **explicit missing_capability** stubs so behavior is
 * clear until a host broker is installed. Harnesses override these globals.
 */
export function registerExtensionTransportHosts(globalTarget, { bridge }) {
  const b = () => bridge();

  const deliverErr = (cid, cap) => {
    b()?.codex_deliver_callback(
      JSON.stringify({
        correlation_id: cid,
        error: {
          code: "missing_capability",
          message: `${cap} not implemented on this host (install a broker or use harness synthetic stubs)`,
          details: { capability: cap },
        },
      }),
    );
  };

  globalTarget.host_websocket_request = (json) => {
    const p = JSON.parse(json);
    deliverErr(p.correlation_id, "host_websocket_request");
  };

  globalTarget.host_tcp_socket = (json) => {
    const p = JSON.parse(json);
    deliverErr(p.correlation_id, "host_tcp_socket");
  };

  globalTarget.host_app_server_rpc = (json) => {
    const p = JSON.parse(json);
    deliverErr(p.correlation_id, "host_app_server_rpc");
  };
}
