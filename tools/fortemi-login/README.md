# fortemi-login

Sign in once with the organization's identity provider, then use the Fortemi
API, MCP server, scripts and agents as yourself. Python 3, standard library
only, plus an optional `keyring` dependency for OS keychain storage.

## Install

```bash
pip install keyring  # recommended: OS keychain storage
chmod +x tools/fortemi-login/fortemi_login.py
```

Without `keyring`, tokens fall back to a file store that requires an explicit
opt-in on every command: `--allow-file-store` (or `FORTEMI_ALLOW_FILE_STORE=1`).
The file is mode `0600` inside a `0700` directory, and a file with any other
mode is refused.

## Configure

Flags or environment, per command:

| Flag | Env | Default |
|---|---|---|
| `--fortemi-url` | `FORTEMI_URL` | `https://fortemi.example.org` |
| `--issuer` | `FORTEMI_ISSUER` | (required) `https://idp.example.org/realms/example` in examples |
| `--client-id` | `FORTEMI_CLIENT_ID` | `fortemi-cli` |

Endpoints are discovered via `/.well-known/openid-configuration`, so no
per-issuer URL wiring is needed.

## Use

```bash
# Interactive sign-in: loopback redirect with PKCE (S256), browser opens
fortemi_login.py --issuer https://idp.example.org/realms/example login

# Headless hosts: device authorization grant
fortemi_login.py --issuer https://idp.example.org/realms/example login --device

# Fresh access token for scripts (refreshes as needed)
curl -H "Authorization: Bearer $(fortemi_login.py token)" \
  https://fortemi.example.org/api/v1/notes

# Who am I?
fortemi_login.py whoami   # GET /api/v1/me

# Sign out: revokes at the IdP, then deletes local tokens
fortemi_login.py logout

# Ready-to-paste MCP configuration (JSON on stdout, hints on stderr)
fortemi_login.py mcp-config --client claude-code
fortemi_login.py mcp-config --client claude-desktop
fortemi_login.py mcp-config --client cursor
fortemi_login.py mcp-config --client vscode
```

## Security notes

- Refresh tokens stay in the OS keychain (macOS Keychain, Windows Credential
  Manager, Secret Service). The file fallback is explicit and permission-pinned.
- `logout` revokes both tokens at the IdP's revocation endpoint before deleting
  local copies.
- Access tokens are short-lived (5–15 minutes in the reference realm); a lost
  token expires on its own. Report a lost device so its sessions can be ended
  at the IdP.

## Test

```bash
cd tools/fortemi-login && python3 -m unittest
```

Covers PKCE generation (including the RFC 7636 test vector), file-mode
enforcement and refusal without the flag, refresh, logout revocation, and
MCP config output.
