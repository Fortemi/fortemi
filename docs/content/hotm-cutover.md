# Verify HotM after a Fortemi bundle cutover

Direct Fortemi health on port 3000 does not prove the browser path on port
4180 works. A bundle recreated under a different Compose project can lose the
network used by a preserved HotM nginx container. Verify both paths before
declaring the cutover healthy. This is the deployment check for #1127;
authenticated notes/events behavior is separately qualified under HotM #304.

## Preserve the upstream network

For the single-project workstation topology, keep using
`docker-compose.workstation.yml` and its UI profile. For independently preserved
HotM containers, use an existing shared network owned outside the bundle's
Compose lifecycle. Inspect the UI and agent-proxy network attachments and select
their common network. On the affected manual installation it is
`fortemi-manual_default`; do not assume that name on another installation.

Set `FORTEMI_HOTM_NETWORK` in the bundle `.env`, then include the checked-in
override for every bundle lifecycle command:

```bash
export FORTEMI_HOTM_NETWORK=fortemi-manual_default
docker network inspect "$FORTEMI_HOTM_NETWORK" >/dev/null
docker compose -f docker-compose.bundle.yml -f docker-compose.hotm-network.yml config --quiet
docker compose -f docker-compose.bundle.yml -f docker-compose.hotm-network.yml pull
docker compose -f docker-compose.bundle.yml -f docker-compose.hotm-network.yml up -d --no-build
```

The override retains the bundle default network and adds the external HotM
network with the `fortemi` upstream alias. Missing network configuration or a
missing external network stops the Compose operation. The external network must
remain present while HotM uses it. Do not run `down -v` during an upgrade.

A one-time `docker network connect --alias fortemi ...` is incident mitigation,
not durable configuration. Older HotM nginx images can retain a resolved upstream
IP after the server is recreated. Once Fortemi becomes ready, restart the UI
container if its upstream is stale, then repeat all checks below. Use the actual
container name, for example `docker restart hotm-ui-manual`. Do not disable
authentication to make a check pass.

## Required cutover checks

Run the read-only check against the containers actually serving the browser:

```bash
python3 scripts/ci/verify-hotm-cutover.py \
  --server fortemi-fortemi-1 \
  --ui hotm-ui-manual \
  --proxy hotm-agent-proxy-manual \
  --base-url http://localhost:4180
```

This requires running containers, a shared network ID and the `fortemi` alias
for each consumer, and JSON responses from both `/health` and
`/api/v1/system/compatibility` through HotM. It never emits container environment
variables or response bodies. Exit 1 means a topology, transport, HTTP or
response-shape failure. Exit 2 means reachable but authorization required;
resolve the session/configuration without labeling it an offline outage.
Exit 0 establishes topology and HTTP reachability only.

Then perform the browser smoke, with a new private window or a new browser
context (no cached page or existing service worker):

1. Open `http://localhost:4180` on the deployment host, or through its explicitly
   configured local tunnel. Verify the displayed **API Connected** state.
   Wait for the initial health request to settle before recording the badge;
   record the observation duration and any failure to converge.
2. Complete the supported sign-in flow if required. Confirm the application can
   read notes for that authorized session and that no repeated 401/410 reconnect
   loop appears. An empty authorized notebook is a valid result; an unauthorized
   empty list is not.
3. Check that health and compatibility requests use the 4180 origin, not a direct
   port-3000 bypass. A 502 through nginx fails the cutover even if port 3000 is
   healthy. Treat genuine transport failure separately from auth-required state.
4. Record UTC time, immutable image digests, Compose files/project, selected
   network, script exit status and the observed browser badge/session outcome in
   the release receipt. Record failures and corrective action, then rerun from a
   fresh context. Do not record tokens, note contents or browser storage.

Do not claim a successful browser smoke from the Python output: it deliberately
reports `browser_session_verified: false`. This host check does not certify
all tenant permissions, hosted auth, SSE payload semantics, or suite portability.

## Failed cutover

If the check fails, stop promotion. Verify the persistent override, network ID,
upstream alias and nginx resolution before changing application settings. Keep
the prior immutable image and database recovery point identified. Apply the
release's migration-specific rollback or forward-recovery procedure; an older
image is not automatically compatible with an upgraded database.
