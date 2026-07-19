# Ollama Connectivity and Network Exposure

Ollama is an HTTP inference service. Prompts, model metadata, and compute
capacity cross this boundary, so its listener must match the intended trust
scope. Ollama listens on loopback by default; `OLLAMA_HOST` changes that
listener. Docker's `host-gateway` alias resolves to a host address that
containers can use.

Choose one of the following profiles. Do not broaden the listener merely to
clear a connection-refused error.

## Local API Outside Docker

Keep Ollama on its default loopback listener when Fortemi also runs directly
on the host:

```bash
OLLAMA_BASE=http://127.0.0.1:11434
curl -fsS http://127.0.0.1:11434/api/version
```

Loopback does not accept connections from normal Docker bridge networks. That
is expected and is the least-exposure configuration for a host-only process.

## Compose-Managed Local Workstation

For a Docker-based development workstation, prefer the compose-managed Ollama
service:

```bash
./workstation up --backend-only
./workstation models pull
```

Fortemi reaches Ollama over the private compose network at
`http://ollama:11434`. The optional host port is published only on
`127.0.0.1` by default. A non-loopback `FORTEMI_OLLAMA_BIND_ADDR` is an
explicit shared-listener decision and requires the controls in
[Remote or Shared Ollama](#remote-or-shared-ollama).

## Linux Docker to Host Ollama

The headless bundle uses `host.docker.internal`, mapped through Docker's
`host-gateway` feature. On Linux, the gateway defaults to an address on
Docker's default bridge, but it can be configured by the Docker daemon. Do not
hard-code a common bridge address.

Inspect the address Docker will use:

```bash
HOST_GATEWAY_IP="$(
  docker network inspect bridge \
    --format '{{(index .IPAM.Config 0).Gateway}}'
)"
test -n "${HOST_GATEWAY_IP}"
printf 'Docker host gateway: %s\n' "${HOST_GATEWAY_IP}"
```

Configure the host Ollama systemd service to listen only on that address:

```bash
sudo mkdir -p /etc/systemd/system/ollama.service.d
printf '[Service]\nEnvironment="OLLAMA_HOST=%s:11434"\n' \
  "${HOST_GATEWAY_IP}" |
  sudo tee /etc/systemd/system/ollama.service.d/override.conf >/dev/null
sudo systemctl daemon-reload
sudo systemctl restart ollama
```

This changes a host network listener. Review the printed address before
running the `sudo` commands. The Fortemi installer reports this recipe but
does not apply it.

Verify both sides:

```bash
# The listener must show the selected gateway address, not a wildcard.
sudo ss -ltnp 'sport = :11434'

# The bundle already supplies the host-gateway mapping.
docker compose -f docker-compose.bundle.yml exec fortemi \
  curl -fsS --max-time 3 \
  http://host.docker.internal:11434/api/version
```

If `host-gateway` is customized, rootless Docker is in use, or policy forbids
a host listener on the bridge, keep Ollama on loopback and use an
access-controlled reverse proxy. Allow only the Fortemi container subnet or
host, and set `OLLAMA_BASE` to that proxy URL.

## Remote or Shared Ollama

Treat a shared Ollama server as a protected inference service:

1. Bind Ollama to one specific private or service interface.
2. Restrict port `11434` with the host/network firewall to known Fortemi
   clients.
3. Prefer a TLS reverse proxy with authentication or mTLS. Ollama's native API
   should not be exposed directly to an untrusted network.
4. Apply request concurrency, body-size, timeout, and rate controls at the
   proxy or scheduler. An authorized client can still exhaust GPU memory,
   CPU, or model-loading capacity.
5. Set Fortemi's outbound destination policy for the exact proxy origin; that
   separate boundary is owned by issue `#920`.

Example with a private listener:

```bash
OLLAMA_HOST=<PRIVATE_SERVICE_IP>:11434 ollama serve
```

Example Fortemi configuration through an authenticated TLS proxy:

```bash
OLLAMA_BASE=https://ollama.internal.example
```

Verify the server certificate and access control from the Fortemi host before
enabling background embedding jobs. Do not include credentials in the URL or
logs.

## Troubleshooting Matrix

| Deployment | Ollama listener | Fortemi URL | Expected exposure |
|---|---|---|---|
| Both processes on host | `127.0.0.1:11434` | `http://127.0.0.1:11434` | Host loopback only |
| Compose-managed workstation | Compose network; host port on `127.0.0.1` | `http://ollama:11434` | Compose peers plus host loopback |
| Linux bundle to host Ollama | Exact Docker host-gateway address | `http://host.docker.internal:11434` | Docker gateway only |
| Intentional shared service | Specific private/service address or protected proxy | Approved `https://` proxy URL | Firewall/proxy allowlist |

The [Ollama FAQ](https://docs.ollama.com/faq) documents the default loopback
listener and `OLLAMA_HOST`. Docker documents
[`host-gateway`](https://docs.docker.com/reference/cli/dockerd/#configure-host-gateway-ip)
and the
[`host.docker.internal` compose mapping](https://docs.docker.com/compose/how-tos/networking/#custom-hosts).
