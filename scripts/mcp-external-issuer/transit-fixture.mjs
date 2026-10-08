// Minimal OpenBao Transit stand-in for the hosted KMS startup canary in the MCP external-issuer test.
//
// Implements only keys/{key}, encrypt/{key} and decrypt/{key} for a derived aes256-gcm96 key,
// with a per-process random key and token. Not a KMS: it exists so hosted startup can run in a
// disposable harness without an OpenBao container. Usage: node transit-fixture.mjs <root>
// where <root> holds server.pem/server.key; writes <root>/transit.json and <root>/transit-token.
import assert from "node:assert/strict";
import { createServer } from "node:https";
import { createCipheriv, createDecipheriv, randomBytes, timingSafeEqual } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";

const [root] = process.argv.slice(2);
assert.match(root, /^\/[^\0]*\/fortemi-mcp-oidc\.[A-Za-z0-9]+$/);
const key = randomBytes(32);
const token = Buffer.from(randomBytes(24).toString("hex"));
writeFileSync(`${root}/transit-token`, token, { flag: "wx", mode: 0o600 });

const reply = (res, status, body) => {
  res.writeHead(status, { "content-type": "application/json" });
  res.end(JSON.stringify(body));
};

function encrypt(plaintext, aad) {
  const iv = randomBytes(12);
  const cipher = createCipheriv("aes-256-gcm", key, iv);
  cipher.setAAD(Buffer.from(aad));
  const body = Buffer.concat([cipher.update(Buffer.from(plaintext, "base64")), cipher.final()]);
  return `vault:v1:${Buffer.concat([iv, body, cipher.getAuthTag()]).toString("base64")}`;
}

function decrypt(ciphertext, aad) {
  const raw = Buffer.from(ciphertext.replace(/^vault:v1:/, ""), "base64");
  const decipher = createDecipheriv("aes-256-gcm", key, raw.subarray(0, 12));
  decipher.setAAD(Buffer.from(aad));
  decipher.setAuthTag(raw.subarray(raw.length - 16));
  return Buffer.concat([decipher.update(raw.subarray(12, raw.length - 16)), decipher.final()]).toString("base64");
}

const server = createServer(
  { key: readFileSync(`${root}/server.key`), cert: readFileSync(`${root}/server.pem`) },
  async (req, res) => {
    try {
      const presented = Buffer.from(String(req.headers["x-vault-token"] || ""));
      if (presented.length !== token.length || !timingSafeEqual(presented, token)) return reply(res, 403, {});
      const match = /^\/v1\/transit\/(keys|encrypt|decrypt)\/([A-Za-z0-9_-]{1,128})$/.exec(req.url);
      if (!match) return reply(res, 404, {});
      if (match[1] === "keys" && req.method === "GET") {
        return reply(res, 200, { data: { type: "aes256-gcm96", derived: true, convergent_encryption: false,
          exportable: false, allow_plaintext_backup: false, deletion_allowed: false, latest_version: 1 } });
      }
      if (req.method !== "POST") return reply(res, 405, {});
      const chunks = [];
      let size = 0;
      for await (const chunk of req) {
        size += chunk.length;
        if (size > 65536) return reply(res, 413, {});
        chunks.push(chunk);
      }
      const body = JSON.parse(Buffer.concat(chunks).toString("utf8"));
      assert.equal(typeof body.associated_data, "string");
      if (match[1] === "encrypt") return reply(res, 200, { data: { ciphertext: encrypt(body.plaintext, body.associated_data) } });
      return reply(res, 200, { data: { plaintext: decrypt(body.ciphertext, body.associated_data) } });
    } catch {
      reply(res, 400, {});
    }
  },
);
server.listen(0, "127.0.0.1", () => {
  writeFileSync(`${root}/transit.json`, JSON.stringify({ addr: `https://127.0.0.1:${server.address().port}/` }), { flag: "wx", mode: 0o600 });
});
process.once("SIGTERM", () => { server.closeAllConnections(); server.close(() => process.exit(0)); });
