// Preload for MCP egress tests (#1171): records every outbound socket and DNS lookup and blocks non-loopback ones.
//
// Loaded with `node --import`. Each attempt is appended as a JSON line to the file named by
// FORTEMI_EGRESS_LOG so the test can assert that nothing dialed a non-local host. Blocking
// non-loopback targets makes the child behave as if it ran on a no-egress network.

import dns from "node:dns";
import fs from "node:fs";
import net from "node:net";

const LOG = process.env.FORTEMI_EGRESS_LOG;
const LOOPBACK = new Set(["127.0.0.1", "::1", "localhost", "::ffff:127.0.0.1"]);

function record(kind, host, port) {
  if (LOG) fs.appendFileSync(LOG, `${JSON.stringify({ kind, host: String(host), port: port ?? null })}\n`);
}

function targetOf(args) {
  const [first, second] = args;
  if (first && typeof first === "object" && !Array.isArray(first)) {
    if (first.path) return { host: "unix", port: null, local: true };
    return { host: first.host ?? "localhost", port: first.port };
  }
  if (typeof first === "string" && Number.isNaN(Number(first))) return { host: "unix", port: null, local: true };
  return { host: typeof second === "string" ? second : "localhost", port: first };
}

const originalConnect = net.Socket.prototype.connect;
net.Socket.prototype.connect = function guardedConnect(...args) {
  const target = targetOf(args);
  if (!target.local) {
    record("connect", target.host, target.port);
    if (!LOOPBACK.has(target.host)) {
      process.nextTick(() => this.destroy(new Error(`egress blocked: ${target.host}`)));
      return this;
    }
  }
  return originalConnect.apply(this, args);
};

const originalLookup = dns.lookup;
dns.lookup = function guardedLookup(hostname, ...rest) {
  record("lookup", hostname, null);
  if (!LOOPBACK.has(hostname)) {
    const callback = rest[rest.length - 1];
    const error = Object.assign(new Error(`egress blocked: ${hostname}`), { code: "ENOTFOUND" });
    process.nextTick(() => callback(error));
    return {};
  }
  return originalLookup.call(this, hostname, ...rest);
};
