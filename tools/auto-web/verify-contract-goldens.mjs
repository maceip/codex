#!/usr/bin/env node
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const goldPath = path.join(__dirname, "contract-golden-samples.json");

const gold = JSON.parse(readFileSync(goldPath, "utf8"));

function checkEnvelope(label, obj, keys) {
  for (const k of keys) {
    if (!(k in obj)) {
      throw new Error(`${label}: missing key ${k}`);
    }
  }
}

for (const [label, v] of Object.entries(gold.wasm_export_responses ?? {})) {
  JSON.stringify(v);
  if (label === "submit_accepted") {
    checkEnvelope(label, v, ["correlation_id", "status"]);
  }
}

for (const [label, v] of Object.entries(gold.host_callback_envelopes ?? {})) {
  const o = structuredClone(v);
  checkEnvelope(label, o, ["correlation_id"]);
  if (o.error) {
    checkEnvelope(`${label}.error`, o.error, ["code", "message"]);
  }
  if (o.payload) {
    checkEnvelope(`${label}.payload`, o.payload, ["kind"]);
  }
}

console.log("verify-contract-goldens: ok");
