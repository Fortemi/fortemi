# Dependabot Sweep — 2026-05-17

**Source**: GitHub Dependabot alerts on `Fortemi/fortemi` default branch
**Snapshot taken at**: 2026-05-17 (post-PR #685 merge, HEAD = 60f96bd)
**Total open**: 30 alerts (8 high, 15 moderate, 7 low)

## Summary by package family

| Package | Ecosystem | Manifest | Direct/Transitive | Alerts | Worst | Patched-to | Notes |
|---|---|---|---|---|---|---|---|
| `openssl` | rust | Cargo.lock | transitive (via reqwest → hyper-tls → native-tls) | 7 | high (4) | 0.10.79 | currently pinned 0.10.76 |
| `hono` | npm | mcp-server/package-lock.json | direct + transitive bumps | 11 | medium (10) + 1 low | 4.12.18 | currently several versions present |
| `rustls-webpki` | rust | Cargo.lock | transitive | 3 | high (DoS) | 0.103.13 | currently 0.103.10 |
| `rand` | rust | Cargo.lock | direct (workspace dep) | 3 (dup advisory) | low | 0.10.1 | currently 0.10.0 alongside 0.8.5/0.9.2 |
| `fast-uri` | npm | mcp-server/package-lock.json | transitive | 2 | high (both) | 3.1.2 | host confusion + path traversal |
| `pillow` | pip | docker/open3d-renderer/requirements.txt | direct | 2 | medium | 12.2.0 | requirements pinned `>=10.0,<12.0` — bump cap |
| `ip-address` | npm | mcp-server/package-lock.json | transitive | 1 | medium | 10.1.1 | XSS in Address6 |
| `@hono/node-server` | npm | mcp-server/package-lock.json | direct | 1 | medium | 1.19.13 | serveStatic slash bypass |

Net: **7 unique packages**. Most remediations are version bumps; only `pillow` requires a manifest constraint change.

## High-severity findings (8)

All are version-bump fixes; no code changes required in fortemi.

| # | Package | CVE/GHSA | Summary | Patched |
|---|---|---|---|---|
| 50 | fast-uri | CVE-2026-6322 | host confusion via percent-encoded `%40`/`%3A` | 3.1.2 |
| 46 | fast-uri | CVE-2026-6321 | path traversal via percent-encoded dot segments | 3.1.1 |
| 41 | openssl | CVE-2026-42327 | UB in `X509Ref::ocsp_responders` for non-UTF-8 OCSP URLs | 0.10.79 |
| 38 | rustls-webpki | GHSA-82j2-j2ch-gfr8 | DoS via panic on malformed CRL BIT STRING | 0.103.13 |
| 36 | openssl | CVE-2026-41676 | `Deriver::derive` buffer overflow on OpenSSL 1.1.1 | 0.10.78 |
| 34 | openssl | CVE-2026-41678 | incorrect bounds assertion in AES key wrap | 0.10.78 |
| 33 | openssl | CVE-2026-41898 | PSK/cookie trampoline memory leak | 0.10.78 |
| 32 | openssl | CVE-2026-41681 | `digest_final()` writes past caller buffer | 0.10.78 |

## Exploitability assessment

Most of these are **theoretical for fortemi's threat model**:

- **fast-uri** (high): only exploitable if fortemi parses untrusted URLs and uses them for allowlist/redirect decisions. `mcp-server` does process external URLs — needs check.
- **openssl** (high cluster): native-tls is used by `reqwest` for outbound HTTPS only. fortemi is a client, not a server, in that path. Attacker would need to control a server fortemi connects to. Patch is still cheap; do it.
- **rustls-webpki** (high DoS): only exploitable on a path that processes attacker-controlled CRLs. fortemi does not currently use webpki for client cert validation. Theoretical.
- **hono cluster** (medium): mcp-server is exposed on port 3001. cookie/cache/serveStatic/JSX paths matter if fortemi uses them. Worth bumping; non-trivial cluster.
- **pillow** (medium): docker/open3d-renderer processes user-supplied 3D assets. PDF trailer loop is DoS; integer overflow could be exploit. Tighten the version pin.
- **rand** (low): unsoundness only with custom logger; not exercised.

## Remediation plan

Order by leverage (single command unblocks many alerts):

1. **`cargo update -p openssl`** to 0.10.79+ → resolves 7 alerts (#41, #34, #33, #32, #36, #35, #45)
2. **`cargo update -p rustls-webpki`** to 0.103.13+ → resolves 3 alerts (#38, #30, #29)
3. **`cargo update -p rand`** to 0.10.1+ → resolves 3 dup alerts (#37, #31, #26)
4. **`cd mcp-server && npm update hono @hono/node-server`** → resolves 12 alerts (the hono cluster)
5. **`cd mcp-server && npm update fast-uri ip-address`** → resolves 3 alerts (#50, #46, #42)
6. **`docker/open3d-renderer/requirements.txt`**: change `Pillow>=10.0,<12.0` → `Pillow>=12.2.0,<13.0` → resolves 2 alerts (#40, #39)

Each is a small, isolated PR. None are coupled.

## Coverage gaps surfaced

- No CI job runs `cargo audit` or `npm audit` — only GitHub-side Dependabot. A CI-side scanner would surface these on PRs instead of post-merge.
- No `cargo-deny` or equivalent advisory database integration.
- `mcp-server` is treated as a sub-project but isn't in the workspace lock-sync flow.
- Pillow constraint `<12.0` blocks the only patched version. Pin-style review is overdue.

## Companion issues filed

See cross-reference list in the security epic (linked from this file once the epic is opened).

## References

- GHSA database: https://github.com/advisories
- `gh api /repos/Fortemi/fortemi/dependabot/alerts?state=open` — raw source for this report
- `cargo tree -i openssl --workspace` — confirms openssl is transitive via reqwest
