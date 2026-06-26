export const ACCESS_TOKEN_PLACEHOLDER = "<ACCESS_TOKEN>";
export const API_KEY_PLACEHOLDER = "<API_KEY>";
export const STREAM_TOKEN_PLACEHOLDER = "<STREAM_TOKEN>";
export const REDACTED_SECRET_PLACEHOLDER = "<REDACTED_SECRET>";

const SECRET_SENTINEL_RE = /SECRET_TEST_[A-Z0-9_]*DO_NOT_LEAK/g;
const CONTROL_CHAR_RE = /[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]/g;
const AUTH_BEARER_RE = /Authorization:\s*Bearer\s+["']?[^"'\s\\]+/gi;
const BARE_BEARER_RE = /\bBearer\s+(?!<ACCESS_TOKEN>)[A-Za-z0-9._~+/=-]{8,}/gi;
const FORTEMI_TOKEN_RE = /\bmm_(?:at|rt|key)_[A-Za-z0-9._~+/=-]+/g;
const PROVIDER_KEY_RE = /\b(?:sk|sk-proj|sk-or|hf)[_-][A-Za-z0-9._~+/=-]+/g;
const SECRET_ASSIGNMENT_RE =
  /\b(client_secret|registration_access_token|api_key|access_token|refresh_token|token|password|passphrase)=([^&\s"'`]+)/gi;
const CREDENTIAL_URL_RE =
  /\b([a-z][a-z0-9+.-]*:\/\/)([^\/\s"'`:@]+):([^@\s"'`]+)@/gi;

export function safeAuthCurlHeader() {
  return `-H "Authorization: Bearer ${ACCESS_TOKEN_PLACEHOLDER}"`;
}

export function pushSafeAuthCurlHeader(parts) {
  parts.push(safeAuthCurlHeader());
}

export function sanitizeMcpText(value) {
  if (typeof value !== "string") return value;

  return value
    .replace(/\r/g, "\n")
    .replace(CONTROL_CHAR_RE, "")
    .replace(SECRET_SENTINEL_RE, REDACTED_SECRET_PLACEHOLDER)
    .replace(AUTH_BEARER_RE, `Authorization: Bearer ${ACCESS_TOKEN_PLACEHOLDER}`)
    .replace(BARE_BEARER_RE, `Bearer ${ACCESS_TOKEN_PLACEHOLDER}`)
    .replace(FORTEMI_TOKEN_RE, (match) =>
      match.startsWith("mm_key_") ? API_KEY_PLACEHOLDER : ACCESS_TOKEN_PLACEHOLDER
    )
    .replace(PROVIDER_KEY_RE, REDACTED_SECRET_PLACEHOLDER)
    .replace(CREDENTIAL_URL_RE, "$1<USERNAME>:<PASSWORD>@")
    .replace(SECRET_ASSIGNMENT_RE, (_match, key) => `${key}=<REDACTED>`);
}

export function sanitizeMcpOutput(value) {
  if (typeof value === "string") {
    return sanitizeMcpText(value);
  }
  if (Array.isArray(value)) {
    return value.map(sanitizeMcpOutput);
  }
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value).map(([key, entry]) => [
        key,
        sanitizeMcpOutput(entry),
      ])
    );
  }
  return value;
}
