/**
 * Phase 3 host HTTP transport — shared by the wasm loader and e2e tests.
 * Implements `host_http_request` contract: buffered `http_response` or streamed `http_stream_chunk`.
 *
 * @typedef {object} HostHttpHooks
 * @property {(envelope: object) => void} deliver - Full callback envelope `{ correlation_id, capability?, payload?, error? }` for wasm `codex_deliver_callback`.
 * @property {(event: string, detail?: object) => void} [trace] - Optional diagnostics (e.g. CLI trace line).
 * @property {typeof fetch} [fetchImpl] - Injected fetch (default `globalThis.fetch`).
 */

/**
 * Execute one host HTTP request and deliver bridge callbacks.
 *
 * @param {object} req - Parsed JSON from wasm `host_http_request` (includes `correlation_id`, `url`, optional `method`, `headers`, `body`, `timeout_ms`, `stream_response`).
 * @param {HostHttpHooks} host
 * @returns {Promise<void>}
 */
export async function runHostHttpRequest(req, host) {
  const cid = req.correlation_id;
  const deliver = (payload) => {
    host.deliver({ correlation_id: cid, capability: "network", payload });
  };
  const deliverErr = (code, message) => {
    host.deliver({
      correlation_id: cid,
      error: { code, message, details: null },
    });
  };
  const trace = host.trace ?? (() => {});

  try {
    const fetchFn = host.fetchImpl ?? globalThis.fetch;
    const signal = req.timeout_ms
      ? AbortSignal.timeout(req.timeout_ms)
      : undefined;
    const resp = await fetchFn(req.url, {
      method: req.method || "GET",
      headers: req.headers || undefined,
      body: req.body || undefined,
      signal,
    });

    if (req.stream_response) {
      trace("host_call:http_stream", { cid, url: req.url });
      if (!resp.body?.getReader) {
        deliver({
          kind: "http_stream_chunk",
          correlation_id: cid,
          chunk: "",
          done: true,
        });
        return;
      }
      const reader = resp.body.getReader();
      const decoder = new TextDecoder();
      while (true) {
        const { done, value } = await reader.read();
        const chunk = done
          ? decoder.decode()
          : decoder.decode(value || new Uint8Array(), { stream: true });
        deliver({
          kind: "http_stream_chunk",
          correlation_id: cid,
          chunk,
          done,
        });
        if (done) {
          break;
        }
      }
      return;
    }

    const body = await resp.text();
    const headers = {};
    resp.headers.forEach((v, k) => {
      headers[k] = v;
    });
    deliver({
      kind: "http_response",
      correlation_id: cid,
      status: resp.status,
      headers,
      body,
    });
  } catch (err) {
    const code =
      err.name === "TimeoutError"
        ? "timeout"
        : err.name === "AbortError"
          ? "cancelled"
          : "internal";
    trace("host_call:http_error", { cid, code, error: err.message });
    deliverErr(code, err.message);
  }
}
