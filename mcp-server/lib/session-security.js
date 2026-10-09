import crypto from "node:crypto";

export function sessionLogId(sessionId) {
  if (!sessionId) return "none";
  return crypto.createHash("sha256").update(String(sessionId)).digest("hex").slice(0, 12);
}

export function sessionPrincipalMatches(session, requestPrincipalFingerprint) {
  return !session?.principalFingerprint || session.principalFingerprint === requestPrincipalFingerprint;
}

export function getSseMessageSessionId({ headerSessionId, querySessionId, legacySse }) {
  if (headerSessionId) return headerSessionId;
  return legacySse ? querySessionId || null : null;
}

export function apiAuthorizationHeader({ transport, requestToken, apiKey }) {
  if (requestToken) return `Bearer ${requestToken}`;
  if (transport !== "http" && apiKey) return `Bearer ${apiKey}`;
  return null;
}
