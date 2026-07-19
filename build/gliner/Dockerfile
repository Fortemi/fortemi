FROM python:3.12-slim@sha256:57cd7c3a7a273101a6485ba99423ee568157882804b1124b4dd04266317710de

WORKDIR /app

# Install system dependencies for torch CPU
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl \
    && rm -rf /var/lib/apt/lists/*

COPY requirements.lock .
RUN pip install --no-cache-dir \
    --require-hashes \
    --only-binary=:all: \
    -r requirements.lock

RUN python -c "import torch; assert torch.__version__ == '2.13.0+cpu'; assert torch.version.cuda is None"

COPY app.py .

# Pre-download model at build time (avoids first-request delay)
ARG GLINER_MODEL=urchade/gliner_large-v2.1
RUN python -c "from gliner import GLiNER; GLiNER.from_pretrained('${GLINER_MODEL}')"

ENV GLINER_MODEL=${GLINER_MODEL}
ENV GLINER_PORT=8090

EXPOSE 8090

HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
    CMD curl --fail http://localhost:8090/health || exit 1

CMD ["python", "app.py"]
