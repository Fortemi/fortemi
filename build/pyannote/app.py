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
import subprocess
import sys
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
    _pipeline = Pipeline.from_pretrained(_model_name, token=hf_token)
    _pipeline.to(torch.device(device))
    elapsed = time.time() - start
    print(f"Pipeline loaded in {elapsed:.1f}s")

    return _pipeline


@app.on_event("startup")
def startup():
    """Load the pipeline at startup. Exit on failure so Docker can restart with fresh state."""
    try:
        load_pipeline()
    except Exception as e:
        print(f"FATAL: Failed to load pipeline: {e}", file=sys.stderr)
        sys.exit(1)


@app.get("/health")
def health():
    if _pipeline is None:
        return PlainTextResponse("loading", status_code=503)
    return {"status": "ok", "model": _model_name, "device": get_device()}


def _normalize_audio(input_path: str) -> str:
    """Normalize audio to WAV 16kHz mono via ffmpeg.

    pyannote can crash with a ValueError when the actual sample count
    doesn't match the expected count (common with VBR/compressed formats).
    Re-encoding to a clean WAV avoids the mismatch entirely.

    Returns the path to the normalized WAV file (caller must delete).
    """
    wav_path = input_path + ".norm.wav"
    try:
        subprocess.run(
            [
                "ffmpeg", "-y", "-i", input_path,
                "-ac", "1", "-ar", "16000", "-sample_fmt", "s16",
                wav_path,
            ],
            capture_output=True,
            timeout=120,
            check=True,
        )
        return wav_path
    except (subprocess.CalledProcessError, FileNotFoundError) as exc:
        # If ffmpeg fails, fall back to the original file
        print(f"WARNING: ffmpeg normalization failed ({exc}), using original audio")
        if os.path.exists(wav_path):
            os.unlink(wav_path)
        return input_path


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

    # Normalize to WAV 16kHz mono to avoid pyannote sample-count mismatches
    norm_path = _normalize_audio(tmp_path)

    try:
        # Build pipeline parameters
        params = {}
        if min_speakers is not None:
            params["min_speakers"] = min_speakers
        if max_speakers is not None:
            params["max_speakers"] = max_speakers

        # Run diarization
        output = pipeline(norm_path, **params)

        # pyannote.audio 4.x returns DiarizeOutput dataclass;
        # the Annotation is in .speaker_diarization
        diarization = getattr(output, "speaker_diarization", output)

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
        if norm_path != tmp_path and os.path.exists(norm_path):
            os.unlink(norm_path)


if __name__ == "__main__":
    port = int(os.environ.get("PYANNOTE_PORT", "8001"))
    uvicorn.run(app, host="0.0.0.0", port=port, log_level="info")
