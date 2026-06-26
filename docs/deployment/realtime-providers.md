# Real-Time Provider Setup: Twilio Voice + Deepgram

This guide connects a Twilio Programmable Voice number to Fortemi's realtime call control path and configures Deepgram as the streaming ASR backend. It is written for the current Fortemi API surface on `main`.

Current implementation summary:

- Twilio media WebSocket endpoint: `GET /api/v1/realtime/twilio/{CallSid}`
- Twilio Voice webhook receiver path: `POST /api/v1/webhooks/incoming/{slug}`
- Call lookup path: `GET /api/v1/calls/{call_id}`
- Supported Twilio receiver schema: `twilio.voice.v1`
- Supported Twilio media schema: `twilio.media-stream.v1`
- Deepgram config is read from `DEEPGRAM_*` env vars.
- Twilio account credentials are not read from process env by the API. Store the Twilio Auth Token in the incoming webhook receiver `hmac_secret`; Fortemi validates `X-Twilio-Signature` with that secret.

## Contract Model

Fortemi keeps the realtime contract provider-neutral so different operators can use the same call lifecycle surface with different telephony, consent, and infrastructure shapes.

| Layer | Stable contract | Provider-specific part |
|---|---|---|
| Receiver registration | `slug`, `provider`, `schema_ref`, `signature_header`, active state, and secret presence | The actual signature algorithm and payload shape, selected by `schema_ref` and `signature_header` |
| Call start | `provider`, `provider_call_id`, optional `remote_party`, metadata, and audit fields | Twilio maps `CallSid`, `From`, `To`, `Direction`, consent, and disclosure fields into that shape |
| Media stream | `MediaFrame` with codec, RTP timestamp, sequence, marker, and payload | Twilio Media Streams JSON envelopes and PCMU/G.711 payloads stay inside the Twilio adapter |
| Call lifecycle | `active`, `ended`, `normal_hangup`, `failed`, and `dropped` | Provider status names such as `ringing`, `in-progress`, `completed`, `busy`, and `no-answer` |
| Transcription backend | Streaming ASR session events: partial, final, and error | Deepgram URL, API key, model, language, reconnect behavior, and fallback accounting |

Recommended deployment profiles:

| Profile | Best fit | Contract choices |
|---|---|---|
| Local developer | Testing adapter logic without public traffic | Use mock adapters, local Deepgram-compatible test endpoints, and disabled confirmation gates. |
| Small team / single tenant | One Twilio number and one Fortemi deployment | Use `twilio-voice-events`, Twilio Auth Token receiver secret, Deepgram env config, and the standard consent metadata fields. |
| Multi-tenant or agency | Multiple customers, numbers, or legal policies | Use separate receiver slugs per tenant or number, separate secrets, tenant-specific disclosure versions, and metadata that identifies the owning account. |
| Regulated operator | Healthcare, finance, public sector, or all-party consent regions | Require explicit confirmation, store disclosure versions, minimize retained transcript fields, and verify vendor, retention, and deletion controls before production. |
| Future provider integration | SIP, LiveKit, or another voice provider | Keep new provider wire fields inside a provider adapter and emit the same call, media, and ASR contracts listed above. |

The slug is an operational routing handle, not the identity boundary by itself. Multi-tenant deployments should carry tenant/account context in receiver configuration, call-session metadata, or an upstream routing layer and should keep secrets separated per tenant or provider account.

Contract completeness checklist:

| Concern | Contract expectation | Why it matters |
|---|---|---|
| Identity | Carry `provider`, `provider_call_id`, internal `call_id`, and optional tenant/account metadata without assuming a single operator. | Local, single-tenant, and multi-tenant users can share the same lifecycle model. |
| Consent and disclosure | Preserve confirmation status, disclosure version, and policy metadata as structured call metadata. | Regulated and all-party-consent deployments can audit decisions without provider-specific schema changes. |
| Lifecycle | Emit standards-shaped `call_started`, `state_change`, `recording_available`, and `ended` call events through the outbox. | Downstream jobs and future providers depend on stable events rather than Twilio status strings. |
| Media | Normalize provider envelopes into `MediaFrame` values before ASR. | Codec and transport choices can evolve independently from transcript persistence. |
| Batch quality path | Treat recording callbacks as a durable `recording_available` contract. | Live transcripts and later higher-quality batch transcripts can coexist under an explicit policy. |

References:

- Twilio Media Streams: https://www.twilio.com/docs/voice/media-streams
- Twilio `<Stream>` TwiML: https://www.twilio.com/docs/voice/twiml/stream
- Twilio webhook security: https://www.twilio.com/docs/usage/webhooks/webhooks-security
- Deepgram authentication: https://developers.deepgram.com/reference/authentication

## Prerequisites

You need:

- A running Fortemi API reachable from Twilio over public HTTPS/WSS.
- A Twilio account with a Voice-capable phone number.
- The Twilio Account SID and Auth Token from the Twilio Console. The Account SID is useful for operations; the Auth Token is required as the Fortemi receiver secret.
- A Deepgram project with an API key that is allowed to use live speech-to-text.
- A decision on call recording/transcription consent for every jurisdiction where callers or operators may be located. See [Consent and Disclosure](#consent-and-disclosure).

## Public Reachability

Twilio must be able to reach both Fortemi endpoints from Twilio's infrastructure:

- Webhook endpoint: `https://<public-host>/api/v1/webhooks/incoming/twilio-voice-events`
- Media stream endpoint: `wss://<public-host>/api/v1/realtime/twilio/{CallSid}`

Use one of these deployment patterns:

| Pattern | Use when | Notes |
|---|---|---|
| Public reverse proxy | Production server | Terminate TLS at nginx, Caddy, Traefik, or an equivalent proxy; forward `Host`, `X-Forwarded-Host`, and `X-Forwarded-Proto`. |
| Cloudflare Tunnel | Private server behind NAT | Configure the tunnel hostname as the public Fortemi base URL. |
| ngrok | Development only | Use a fixed paid domain if you need stable Twilio webhook configuration. |

Fortemi validates Twilio webhook signatures against the externally visible URL. If a proxy rewrites the host or scheme, pass the original values with `X-Forwarded-Host` and `X-Forwarded-Proto`; otherwise Twilio signature validation will fail.

## Deepgram Configuration

Set Deepgram credentials in the Fortemi API environment. Prefer secret files for deployed systems.

```bash
# Preferred in containers or secret-mounted deployments
DEEPGRAM_API_KEY_FILE=/run/secrets/deepgram_api_key

# Acceptable for local development only
# DEEPGRAM_API_KEY=dg_...

DEEPGRAM_MODEL=nova-3
DEEPGRAM_LANGUAGE=en
DEEPGRAM_ENCODING=linear16
DEEPGRAM_SAMPLE_RATE_HZ=16000

# Optional: enables fallback accounting in the Deepgram backend when a fallback is configured.
# REALTIME_ASR_BACKEND_FALLBACK=mock
```

`DEEPGRAM_LISTEN_URL` defaults to `wss://api.deepgram.com/v1/listen`. Override it only for a local mock server or private Deepgram-compatible endpoint.

Operational controls:

- Configure Deepgram project limits and billing alerts in Deepgram's console before production traffic.
- Keep the API key server-side. Do not place it in TwiML, browser code, or mobile clients.
- Rotate the key if it appears in logs, terminal history, or committed files.

## Register The Twilio Voice Receiver

Create the Phase B incoming webhook receiver. Use the Twilio Auth Token as `hmac_secret`; Fortemi uses it to validate `X-Twilio-Signature`.

```bash
curl -sS -X POST http://localhost:3000/api/v1/webhooks/incoming \
  -H 'Content-Type: application/json' \
  -d '{
    "slug": "twilio-voice-events",
    "provider": "twilio",
    "schema_ref": "twilio.voice.v1",
    "hmac_secret": "<twilio-auth-token>",
    "signature_header": "X-Twilio-Signature",
    "is_active": true
  }'
```

Confirm registration:

```bash
curl -sS http://localhost:3000/api/v1/webhooks/incoming/twilio-voice-events
```

The response intentionally reports `secret_set: true` rather than returning the secret.

## Configure TwiML

Twilio sends lifecycle callbacks to the receiver URL and opens the media WebSocket to Fortemi. Use a TwiML Bin, Twilio Function, or your own TwiML endpoint.

Minimal bidirectional stream TwiML:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Response>
  <Connect>
    <Stream url="wss://fortemi.example.com/api/v1/realtime/twilio/{CallSid}" />
  </Connect>
</Response>
```

For production, add a disclosure before the stream starts:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Response>
  <Say voice="alice">This call may be recorded and transcribed. If you do not consent, please hang up now.</Say>
  <Pause length="1" />
  <Connect>
    <Stream url="wss://fortemi.example.com/api/v1/realtime/twilio/{CallSid}" />
  </Connect>
</Response>
```

If you require explicit confirmation, collect it before streaming and include `ConsentConfirmed=true` when your TwiML endpoint posts or redirects into Fortemi's call-start receiver. A Twilio Function or self-hosted TwiML endpoint is usually easier than a static TwiML Bin for this flow. Minimal sketch:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Response>
  <Gather numDigits="1" action="https://voice.example.com/consent" method="POST">
    <Say voice="alice">This call may be recorded and transcribed. Press 1 to consent, or hang up now.</Say>
  </Gather>
  <Hangup />
</Response>
```

The `/consent` handler should only return `<Connect><Stream>` after it has recorded the confirmation and posted a call-start webhook body containing `ConsentConfirmed=true`, `DisclosurePlayed=true`, and `DisclosureVersion=<your-version>`.

Configure the Twilio phone number voice webhook:

- Method: `POST`
- URL: your TwiML Bin, Twilio Function, or self-hosted TwiML endpoint
- Status callback URL: `https://fortemi.example.com/api/v1/webhooks/incoming/twilio-voice-events`
- Status callback method: `POST`

Fortemi expects Twilio's standard URL-encoded Voice webhook fields, including `CallSid`, `CallStatus`, `From`, `To`, and `Direction`. Recording callbacks may include `RecordingSid`, `RecordingStatus`, and `RecordingUrl`.

For multi-tenant deployments, use a distinct receiver slug per tenant or number when secrets, disclosure policies, retention, or billing boundaries differ. Keep the Twilio Account SID and phone-number ownership in your operations inventory or tenant metadata; the Fortemi realtime adapter only needs the signed webhook body and `CallSid` correlation contract.

## First Call Walkthrough

1. Start Fortemi and confirm the base health endpoint is healthy.

   ```bash
   curl -fsS http://localhost:3000/health
   ```

2. Confirm the Twilio receiver exists.

   ```bash
   curl -fsS http://localhost:3000/api/v1/webhooks/incoming/twilio-voice-events
   ```

3. Dial the Twilio number.

4. Watch Fortemi logs for these events:

   - Twilio Voice webhook accepted.
   - Call session created for provider `twilio` and the Twilio `CallSid`.
   - Twilio media WebSocket accepted on `/api/v1/realtime/twilio/{CallSid}`.
   - Media frames received.

5. Use the call ID from logs or the webhook response side effect to fetch the call.

   ```bash
   curl -sS 'http://localhost:3000/api/v1/calls/<call_id>?limit=50&offset=0'
   ```

The call detail response includes the provider call ID, timestamps, end reason, ASR backend, remote party, and persisted final transcript segments when transcript persistence is enabled for the session.

## Event Mapping

Fortemi maps Twilio Voice statuses into standards-shaped call events at the adapter boundary.

| Twilio input | Fortemi side effect |
|---|---|
| `CallStatus=ringing` | Create or reuse a `twilio` call session; remote party is taken from `From` or `To` depending on direction. |
| `CallStatus=answered` or `in-progress` | Treat the session as active. |
| `CallStatus=completed` | End the session with `normal_hangup`. |
| `CallStatus=failed`, `busy`, or `no-answer` | End the session with `failed`. |
| `RecordingStatus=completed` + `RecordingUrl` | Return a `recording_available` side effect for downstream transcription handling. |
| Twilio Media Streams `media` envelope | Translate PCMU/G.711 8 kHz payloads into Fortemi `MediaFrame` values. |
| Twilio Media Streams `stop` or socket close | End the call as dropped if no terminal webhook already ended it. |

## Consent and Disclosure

Call recording and live transcription laws vary by jurisdiction. Fortemi provides the transport and persistence tools; operators are responsible for lawful use.

Minimum production practice:

- Play a disclosure before media streaming or recording starts.
- Record which disclosure text/version was played for each deployment window.
- Provide an opt-out path before streaming starts, such as hanging up, pressing a key, or routing to a non-recorded line.
- Disable speaker identification or voice biometric processing unless your legal basis covers it.
- Keep retention, access control, and deletion policy aligned with your regulatory environment.

Reference points operators must verify with counsel:

| Area | Practical implication |
|---|---|
| US federal law | Often described as one-party consent, but state law may impose stricter requirements. |
| US all-party/two-party consent states | States including California, Florida, Illinois, Maryland, Massachusetts, Montana, Nevada, New Hampshire, Pennsylvania, and Washington may require consent from all parties in common call-recording scenarios. Verify the current list before deployment. |
| EU GDPR and ePrivacy | Plan for a lawful basis, explicit notice, data minimization, retention limits, access rights, and processor/vendor terms. |
| Illinois BIPA and similar biometric laws | Speaker identification or voiceprint features can introduce biometric-specific consent and retention obligations. |
| HIPAA | Healthcare deployments that may include PHI need HIPAA-specific administrative, technical, and vendor controls. |

Fortemi can enforce a call-session gate when your TwiML or call-routing layer posts consent metadata before streaming starts. Configure these API environment variables when you want Fortemi to reject Twilio media WebSocket binding unless consent was confirmed on the Voice webhook that creates the call session:

```bash
# Text/version are copied into call-session metadata for audit.
FORTEMI_CALL_RECORDING_DISCLOSURE_TEXT="This call may be recorded and transcribed. If you do not consent, please hang up now."
FORTEMI_CALL_RECORDING_DISCLOSURE_VERSION="voice-disclosure-2026-05"

# When true, Fortemi will not create the Twilio call session unless the
# call-start webhook includes ConsentConfirmed=true or RecordingConsent=true.
# This is a strict security/privacy boolean: use exactly true, false, 1, or 0.
# Invalid values fail API startup instead of falling back to false.
FORTEMI_CALL_RECORDING_REQUIRE_CONFIRMATION=false
```

When `FORTEMI_CALL_RECORDING_REQUIRE_CONFIRMATION=true`, the Twilio Voice webhook that starts the call must include one of these URL-encoded fields with a truthy value (`true`, `yes`, `confirmed`, or `1`):

- `ConsentConfirmed`
- `RecordingConsent`

Optional audit fields:

- `DisclosurePlayed=true` records that your TwiML/call router played the disclosure before posting the call-start event.
- `DisclosureVersion=<version>` records the exact disclosure copy or policy version used for the call.

If confirmation is required and absent, Fortemi returns a `call_session_blocked_consent_required` side effect and does not create the call session. The later `/api/v1/realtime/twilio/{CallSid}` WebSocket upgrade is then rejected because the recent session-token gate has no matching session.

## Troubleshooting

### Twilio Webhook Returns 401

Likely causes:

- Receiver `signature_header` is not `X-Twilio-Signature`.
- Receiver `hmac_secret` does not match the Twilio Auth Token.
- Reverse proxy does not preserve the public host or scheme. Forward `X-Forwarded-Host` and `X-Forwarded-Proto`.
- Twilio is posting to a different URL than the one Fortemi reconstructs.

### Twilio WebSocket Does Not Connect

Check:

- The URL uses `wss://`, not `https://`.
- The public hostname routes WebSocket upgrades to Fortemi.
- The Twilio Voice webhook created a call session within the recent binding window before `<Stream>` connects.
- The path includes the Twilio `CallSid`: `/api/v1/realtime/twilio/{CallSid}`.

### Deepgram Auth Fails

Check:

- `DEEPGRAM_API_KEY` or `DEEPGRAM_API_KEY_FILE` is set in the Fortemi API process.
- The file path is readable by the container or service user.
- The key has not been revoked and the Deepgram project has live transcription access.
- The API key is not accidentally quoted with trailing whitespace.

### Calls Exist But No Transcript Segments Appear

Check:

- The call session was created and media frames are reaching the Twilio WebSocket endpoint.
- Deepgram configuration is present and valid.
- Transcript persistence is enabled in the deployed realtime pipeline. Partial ASR hypotheses may remain ephemeral; `GET /api/v1/calls/{call_id}` returns persisted final transcript segments.

### Recording Callback Arrives But No Transcription Job Starts

Fortemi recognizes `recording.completed` callbacks, emits a durable `recording_available` call event, imports the recording into file storage when file storage is configured, creates a call-recording note/attachment, and queues the existing `AudioTranscription` job with `parent_attachment_id` and `audio_attachment_id`. The batch transcript policy is `append_attachment_transcript`: live realtime segments stay in `transcript_segments`, while the higher-quality batch transcript follows the existing attachment transcript/caption path and is linked back to the call through call-session metadata.

Check:

- File storage is configured for the API process; without it Fortemi cannot create the audio attachment required by `AudioTranscriptionHandler`.
- The Twilio `RecordingUrl` is reachable by the API process.
- The recording response has non-empty audio content and a supported audio content type.
- The worker tier that handles `audio_transcription` jobs is running and has a healthy transcription backend.
