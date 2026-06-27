# Extraction Services Deployment Guide

This guide covers deploying extraction services for Fortemi.

## Overview

Fortemi supports multiple extraction services for processing different file types:

- **Vision Model** — Extract text and metadata from images
- **Whisper Transcription** — Transcribe audio files
- **Speaker Diarization** — Identify and label speakers in audio/video
- **GLiNER NER** — Zero-shot named entity recognition
- **Media Optimization** — Pre-generate streaming-friendly media variants
- **OCR** — Extract text from scanned documents
- **LibreOffice** — Convert office documents to text

The Docker bundle (`docker-compose.bundle.yml`) includes all services with sensible defaults. Individual services can be disabled by setting their URL to empty.

## Vision Model

### Requirements

- Ollama installed on the host
- Vision model pulled (e.g., `qwen3.5:9b` — natively multimodal, unified generation and vision)

### Setup

1. Pull the vision model on the host:
   ```bash
   ollama pull qwen3.5:9b
   ```

2. Configure in `.env`:
   ```bash
   OLLAMA_VISION_MODEL=qwen3.5:9b
   ```

3. Restart the bundle:
   ```bash
   docker compose -f docker-compose.bundle.yml down
   docker compose -f docker-compose.bundle.yml up -d
   ```

### Verification

Check that the API can access the vision model:
```bash
curl http://localhost:3000/health
```

The health endpoint should show the vision model is available.

## Whisper Transcription

### Requirements

- Docker with NVIDIA GPU support
- NVIDIA Container Toolkit installed
- CUDA 12.6.3 compatible GPU

### Setup

1. Deploy the Whisper service:
   ```bash
   docker compose -f docker-compose.whisper.yml up -d
   ```

2. Verify the service is running:
   ```bash
   curl http://localhost:8000/health
   ```

3. Configure Fortemi to use the service in `.env`:
   ```bash
   WHISPER_BASE_URL=http://host.docker.internal:8000
   WHISPER_MODEL=Systran/faster-distil-whisper-large-v3
   ```

4. Restart the bundle:
   ```bash
   docker compose -f docker-compose.bundle.yml down
   docker compose -f docker-compose.bundle.yml up -d
   ```

### Testing Transcription

Test the Whisper service directly:
```bash
curl -X POST http://localhost:8000/v1/audio/transcriptions \
  -F "file=@audio.mp3" \
  -F "model=Systran/faster-distil-whisper-large-v3"
```

### Model Selection

The default model is `Systran/faster-distil-whisper-large-v3`, which provides a good balance of speed and accuracy. Other options:

- `openai/whisper-large-v3` - Highest accuracy, slower
- `openai/whisper-medium` - Medium accuracy, faster
- `openai/whisper-small` - Lower accuracy, very fast

Configure via the `WHISPER_MODEL` environment variable.

### Resource Requirements

- GPU Memory: 4-8 GB VRAM (depends on model)
- Disk Space: 2-6 GB for model weights
- CPU: 4+ cores recommended

### Performance

On NVIDIA RTX 3090:
- `whisper-large-v3`: ~2-3x realtime
- `faster-distil-whisper-large-v3`: ~5-10x realtime

### Troubleshooting

#### Service won't start

Check GPU availability:
```bash
nvidia-smi
docker run --rm --gpus all nvidia/cuda:12.6.3-base-ubuntu22.04 nvidia-smi
```

#### Out of memory

Reduce model size or batch size in the service configuration.

#### Slow transcription

Check that GPU is being used (not CPU fallback):
```bash
docker logs speaches | grep -i cuda
```

## Speaker Diarization (pyannote)

Speaker diarization identifies and labels different speakers in audio and video transcripts.

### Requirements

- Docker with NVIDIA GPU support (or CPU-only profile)
- HuggingFace token for gated pyannote models (first download only)

### Setup

The Docker bundle includes the pyannote sidecar by default. It starts automatically alongside the main container.

1. Set your HuggingFace token for first-time model download:
   ```bash
   # .env
   HF_TOKEN=<HF_TOKEN>
   ```

2. Optionally configure the diarization model:
   ```bash
   DIARIZATION_BASE_URL=http://pyannote:8001   # Default in bundle
   DIARIZATION_MODEL=pyannote/speaker-diarization-3.1
   ```

3. For CPU-only environments:
   ```bash
   docker compose -f docker-compose.bundle.yml --profile pyannote-cpu up -d
   ```

### Disabling

Set `DIARIZATION_BASE_URL=` (empty) in `.env` to disable diarization entirely.

### What It Produces

After audio/video transcription, diarization adds:
- Speaker labels to VTT/SRT/TXT caption files
- A speaker configuration block in note content
- Editable speaker names (triggers `SpeakerRelabel` job on save)

## GLiNER NER (Named Entity Recognition)

Zero-shot named entity recognition for concept tagging. Runs as a CPU-only sidecar container.

### Setup

Included in the Docker bundle by default:

```bash
GLINER_BASE_URL=http://gliner:8090  # Default in bundle
GLINER_MODEL=urchade/gliner_large-v2.1
GLINER_THRESHOLD=0.3
```

### Performance

- **Speed**: <300ms per document (CPU-only, no GPU required)
- **Memory**: ~2 GB RAM for the 0.5B BERT model
- **Role**: Tier-0 in the concept tagging pipeline (runs before LLM extraction)

### Disabling

Set `GLINER_BASE_URL=` (empty) in `.env` to skip NER and use LLM-only concept extraction.

## Media Optimization (ffmpeg)

Pre-generates streaming-friendly media variants during attachment upload.

### Requirements

- `ffmpeg` and `ffprobe` installed in the container (included in the Docker bundle)

### Configuration

Media optimization is enabled by default for video/audio uploads via the `media_optimize` flag. No additional environment variables are required.

### What It Produces

Depending on the source media, the handler generates:

| Variant | Applies To | Description |
|---------|-----------|-------------|
| `faststart` | Video (non-faststart MP4) | Moov atom moved to front for progressive download |
| `web_compatible` | Video (non-H.264/AAC) | Remuxed/transcoded to H.264+AAC MP4 |
| `audio_only` | Video | Extracted audio track in M4A container |
| `preview_720p` | Video (>720p) | Downscaled 720p preview |
| `web_audio` | Audio (non-AAC/MP3/Opus) | AAC transcode in M4A container |
| `audio_preview` | Audio (lossless) | Lossy AAC preview of FLAC/ALAC/WAV source |

Access variants via `GET /api/v1/attachments/{id}/download?variant=web_compatible`.

## Network Configuration

### Same Docker Network (Recommended)

If running both services on the same Docker network, use service names:
```bash
WHISPER_BASE_URL=http://whisper:8000
```

### Host Network

If using `host.docker.internal`:
```bash
WHISPER_BASE_URL=http://host.docker.internal:8000
```

Ensure `extra_hosts` is configured in `docker-compose.bundle.yml`:
```yaml
extra_hosts:
  - "host.docker.internal:host-gateway"
```

## Production Deployment

### Nginx Reverse Proxy

Configure nginx to route extraction service traffic:

```nginx
# Internal extraction services (not exposed publicly)
location /whisper/ {
    proxy_pass http://localhost:8000/;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
}
```

### Security

- Do NOT expose Whisper service publicly (no authentication)
- Use firewall rules to restrict access to localhost only
- Run behind Fortemi API for authentication/authorization

### Monitoring

Monitor service health:
```bash
# Whisper health
curl http://localhost:8000/health

# Check logs
docker logs speaches --tail 100
```

### Backup and Recovery

Model weights are stored in named volumes:
```bash
# Backup model weights
docker run --rm -v speaches-models:/data -v $(pwd):/backup \
  ubuntu tar czf /backup/speaches-models.tar.gz /data

# Restore
docker run --rm -v speaches-models:/data -v $(pwd):/backup \
  ubuntu tar xzf /backup/speaches-models.tar.gz -C /
```

## Cost Optimization

### GPU Sharing

Multiple services can share the same GPU with MIG (Multi-Instance GPU) on supported NVIDIA GPUs.

### Auto-scaling

For cloud deployments, use auto-scaling based on queue depth:
```yaml
deploy:
  replicas: 1
  resources:
    reservations:
      devices:
        - driver: nvidia
          count: 1
          capabilities: [gpu]
  update_config:
    parallelism: 1
    delay: 10s
  restart_policy:
    condition: on-failure
```

## Environment Variables Reference

### Vision Model
- `OLLAMA_VISION_MODEL` — Vision model name (e.g., `qwen3.5:9b`). Set to empty to disable. qwen3.5:9b is natively multimodal (unified generation and vision).

### Whisper Transcription
- `WHISPER_BASE_URL` — Whisper service URL (e.g., `http://whisper:8000`). Set to empty to disable.
- `WHISPER_MODEL` — Whisper model name (default: `Systran/faster-distil-whisper-large-v3`)

### Speaker Diarization
- `DIARIZATION_BASE_URL` — pyannote service URL (e.g., `http://pyannote:8001`). Set to empty to disable.
- `DIARIZATION_MODEL` — Diarization model (default: `pyannote/speaker-diarization-3.1`)
- `HF_TOKEN` — HuggingFace token for gated model download (first time only)

### GLiNER NER
- `GLINER_BASE_URL` — GLiNER service URL (e.g., `http://gliner:8090`). Set to empty to disable.
- `GLINER_MODEL` — GLiNER model (default: `urchade/gliner_large-v2.1`)
- `GLINER_THRESHOLD` — Entity confidence threshold (default: `0.3`)

### OCR
- `OCR_ENABLED` — Enable OCR processing (default: `false`)

### LibreOffice
- `LIBREOFFICE_PATH` — Path to LibreOffice binary (e.g., `/usr/bin/libreoffice`)

## Integration with Fortemi

Extraction services are automatically used when:

1. File is uploaded via API
2. Document type is detected
3. Appropriate extractor is configured
4. Job is enqueued for processing

Monitor extraction jobs:
```bash
# Check job queue
curl http://localhost:3000/api/v1/jobs

# Check specific job status
curl http://localhost:3000/api/v1/jobs/{job_id}
```

## References

- [Speaches Documentation](https://github.com/speaches-ai/speaches)
- [Ollama Vision Models](https://ollama.com/library)
- [pyannote Speaker Diarization](https://github.com/pyannote/pyannote-audio)
- [GLiNER NER](https://github.com/urchade/GLiNER)
- [Job Monitoring Guide](../content/job-monitoring.md) — Progress tracking for extraction and media optimization jobs
