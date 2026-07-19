-- Authoritative realtime ASR media ownership and accepted-sample accounting.
--
-- Provider stream identifiers are stored only as SHA-256 digests. The internal
-- call UUID and monotonically increasing attempt number are Fortemi's identity.

CREATE TABLE public.realtime_media_stream_attempt (
    attempt_id UUID PRIMARY KEY,
    call_id UUID NOT NULL REFERENCES public.call_sessions(call_id) ON DELETE CASCADE,
    attempt_number INTEGER NOT NULL CHECK (attempt_number > 0),
    claim_id UUID NOT NULL UNIQUE,
    provider_binding_sha256 BYTEA NOT NULL
        CHECK (octet_length(provider_binding_sha256) = 32),
    sample_rate_hz INTEGER NOT NULL
        CHECK (sample_rate_hz BETWEEN 1 AND 384000),
    accepted_samples BIGINT NOT NULL DEFAULT 0
        CHECK (accepted_samples >= 0),
    status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN (
            'active',
            'completed',
            'client_interrupted',
            'provider_interrupted',
            'start_failed',
            'close_failed',
            'failover'
        )),
    claimed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_sample_at TIMESTAMPTZ,
    terminal_at TIMESTAMPTZ,
    UNIQUE (call_id, attempt_number),
    UNIQUE (call_id, provider_binding_sha256),
    CHECK (
        (status = 'active' AND terminal_at IS NULL)
        OR (status <> 'active' AND terminal_at IS NOT NULL)
    )
);

CREATE UNIQUE INDEX uq_realtime_media_stream_active_call
    ON public.realtime_media_stream_attempt (call_id)
    WHERE status = 'active';

CREATE INDEX idx_realtime_media_stream_attempt_call
    ON public.realtime_media_stream_attempt (call_id, attempt_number DESC);

CREATE OR REPLACE FUNCTION public.enforce_realtime_media_stream_attempt_transition()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.status <> 'active' THEN
        RAISE EXCEPTION 'terminal realtime media stream attempts are immutable';
    END IF;

    IF NEW.attempt_id <> OLD.attempt_id
        OR NEW.call_id <> OLD.call_id
        OR NEW.attempt_number <> OLD.attempt_number
        OR NEW.claim_id <> OLD.claim_id
        OR NEW.provider_binding_sha256 <> OLD.provider_binding_sha256
        OR NEW.sample_rate_hz <> OLD.sample_rate_hz
        OR NEW.claimed_at <> OLD.claimed_at
    THEN
        RAISE EXCEPTION 'realtime media stream attempt identity is immutable';
    END IF;

    IF NEW.accepted_samples < OLD.accepted_samples THEN
        RAISE EXCEPTION 'accepted realtime samples cannot decrease';
    END IF;

    RETURN NEW;
END;
$$;

CREATE TRIGGER trg_realtime_media_stream_attempt_transition
BEFORE UPDATE ON public.realtime_media_stream_attempt
FOR EACH ROW
EXECUTE FUNCTION public.enforce_realtime_media_stream_attempt_transition();
