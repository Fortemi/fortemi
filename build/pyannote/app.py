"""
pyannote speaker diarization REST API sidecar.

Provides a /diarize endpoint that accepts audio files and returns RTTM output,
and a /health endpoint for container orchestration.

Environment variables:
  PYANNOTE_MODEL: Model name (default: pyannote/speaker-diarization-3.1)
  PYANNOTE_PORT: Server port (default: 8001)
  PYANNOTE_DEVICE: torch device (default: auto → cuda if available, else cpu)
  HF_TOKEN: HuggingFace token for gated models (required for first download)
"""

import io
import os
import tempfile
import time

import torch
import uvicorn
from fastapi import FastAPI, File, Form, UploadFile
from fastapi.responses import PlainTextResponse

app = FastAPI(title="pyannote-diarization-sidecar")

# Global pipeline reference (loaded once at startup)
_pipeline = None
_model_name = os.environ.get("PYANNOTE_MODEL", "pyannote/speaker-diarization-3.1")
_device = os.environ.get("PYANNOTE_DEVICE", "auto")


def get_device():
    if _device == "auto":
        return "cuda" if torch.cuda.is_available() else "cpu"
    return _device


def load_pipeline():
    global _pipeline
    if _pipeline is not None:
        return _pipeline

    from pyannote.audio import Pipeline

    hf_token = os.environ.get("HF_TOKEN")
    device = get_device()

    print(f"Loading pipeline: {_model_name} on {device}")
    start = time.time()
    _pipeline = Pipeline.from_pretrained(_model_name, use_auth_token=hf_token)
    _pipeline.to(torch.device(device))
    elapsed = time.time() - start
    print(f"Pipeline loaded in {elapsed:.1f}s")

    return _pipeline


@app.on_event("startup")
def startup():
    """Pre-load the pipeline at startup to avoid first-request latency."""
    try:
        load_pipeline()
    except Exception as e:
        print(f"WARNING: Failed to pre-load pipeline: {e}")
        print("Pipeline will be loaded on first request.")


@app.get("/health")
def health():
    return {
        "status": "ok" if _pipeline is not None else "loading",
        "model": _model_name,
        "device": get_device(),
    }


@app.post("/diarize", response_class=PlainTextResponse)
async def diarize(
    file: UploadFile = File(...),
    model: str = Form(default=None),
    min_speakers: int = Form(default=None),
    max_speakers: int = Form(default=None),
):
    """
    Run speaker diarization on an uploaded audio file.

    Returns RTTM format (one line per speaker segment):
      SPEAKER <file> 1 <start> <duration> <NA> <NA> <speaker_id> <NA> <NA>
    """
    pipeline = load_pipeline()

    # Write uploaded audio to a temp file (pyannote needs a file path)
    audio_data = await file.read()
    suffix = os.path.splitext(file.filename or "audio.wav")[1] or ".wav"
    with tempfile.NamedTemporaryFile(suffix=suffix, delete=False) as tmp:
        tmp.write(audio_data)
        tmp_path = tmp.name

    try:
        # Build pipeline parameters
        params = {}
        if min_speakers is not None:
            params["min_speakers"] = min_speakers
        if max_speakers is not None:
            params["max_speakers"] = max_speakers

        # Run diarization
        diarization = pipeline(tmp_path, **params)

        # Convert to RTTM format
        rttm_lines = []
        for turn, _, speaker in diarization.itertracks(yield_label=True):
            start = turn.start
            duration = turn.duration
            rttm_lines.append(
                f"SPEAKER {file.filename or 'audio'} 1 {start:.3f} {duration:.3f} "
                f"<NA> <NA> {speaker} <NA> <NA>"
            )

        return "\n".join(rttm_lines) + "\n"

    finally:
        os.unlink(tmp_path)


if __name__ == "__main__":
    port = int(os.environ.get("PYANNOTE_PORT", "8001"))
    uvicorn.run(app, host="0.0.0.0", port=port, log_level="info")
