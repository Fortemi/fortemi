# Shared-Host GPU Test Capacity

Titan is shared with personal vLLM, Ollama and speech workloads. A green CPU or
container check is not permission to load another model into an occupied GPU.
The operator reported repeated terminal lockups and subsequent vLLM cleanup;
the exact cause is not proven by kernel evidence.

The main workflow's GPU job now fails before querying Ollama unless
`scripts/ci/check-gpu-test-capacity.py` sees exactly one GPU with at least
16 GiB free VRAM and 16 GiB available host RAM. It repeats the check before
the integration command. Failed, unavailable, malformed, or ambiguous observations
are failures, not passing skips. Four offline unit tests run in the lint job.
Cargo build concurrency for the GPU job is two; this environment setting is not
a host CPU or RAM hard limit.

The check never loads/unloads models or stops/restarts services. It is a
point-in-time guard, not a GPU lease or kernel-enforced allocation limit.
Operators must coordinate the capacity window and prevent competing model loads
throughout inference acceptance. Capacity may change after the check; this guard
does not establish exclusive scheduling or eliminate all contention risk.

Observed on 2026-09-11 local time: only 5,073 MiB free VRAM; the guard refused
with exit 1 without contacting Ollama or changing vLLM. The suite's recent
OmniCoder-9B FP8 candidate is staged separately, not runtime-qualified. This
change does not silently switch the existing Ollama integration model/backend
or claim acceptance of the new candidate. A coordinated model/backend transition
and actual released-runtime tests remain required. Do not lower the gate just
to obtain a green release or count the refusal as GPU test coverage.
