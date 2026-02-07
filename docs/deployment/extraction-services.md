# Extraction Services Deployment Guide

This guide covers deploying optional extraction services for Fortemi (issues #101 and #102).

## Overview

Fortemi supports multiple extraction services for processing different file types:

- **Vision Model** (Issue #101) - Extract text and metadata from images
- **Whisper Transcription** (Issue #102) - Transcribe audio files
- **OCR** - Extract text from scanned documents
- **LibreOffice** - Convert office documents to text

## Vision Model (Issue #101)

### Requirements

- Ollama installed on the host
- Vision model pulled (e.g., `qwen3-vl:8b`)

### Setup

1. Pull the vision model on the host:
   ```bash
   ollama pull qwen3-vl:8b
   ```

2. Configure in `.env`:
   ```bash
   OLLAMA_VISION_MODEL=qwen3-vl:8b
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

## Whisper Transcription (Issue #102)

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

## Network Configuration

### Same Docker Network (Recommended)

If running both services on the same Docker network, use service names:
```bash
WHISPER_BASE_URL=http://speaches:8000
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
- `OLLAMA_VISION_MODEL` - Vision model name (e.g., `qwen3-vl:8b`)

### Whisper Transcription
- `WHISPER_BASE_URL` - Whisper service URL (e.g., `http://localhost:8000`)
- `WHISPER_MODEL` - Whisper model name (default: `Systran/faster-distil-whisper-large-v3`)

### OCR
- `OCR_ENABLED` - Enable OCR processing (default: `false`)

### LibreOffice
- `LIBREOFFICE_PATH` - Path to LibreOffice binary (e.g., `/usr/bin/libreoffice`)

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

- Issue #101: Deploy qwen3-vl:8b Vision Model
- Issue #102: Deploy faster-whisper-server (Speaches)
- [Speaches Documentation](https://github.com/speaches-ai/speaches)
- [Ollama Vision Models](https://ollama.com/library)
