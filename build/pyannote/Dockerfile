FROM python:3.12-slim@sha256:57cd7c3a7a273101a6485ba99423ee568157882804b1124b4dd04266317710de

WORKDIR /app

# Install system dependencies (ffmpeg for audio decoding, curl for healthcheck)
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl \
    ffmpeg \
    libsndfile1 \
    && rm -rf /var/lib/apt/lists/*

COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY app.py .

ENV PYANNOTE_PORT=8001
ENV PYANNOTE_MODEL=pyannote/speaker-diarization-3.1
ENV PYANNOTE_DEVICE=auto

EXPOSE 8001

HEALTHCHECK --interval=30s --timeout=5s --start-period=120s --retries=3 \
    CMD curl --fail http://localhost:${PYANNOTE_PORT}/health || exit 1

CMD ["python", "app.py"]
