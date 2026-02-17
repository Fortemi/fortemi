FROM python:3.12-slim

WORKDIR /app

# Install system dependencies for torch CPU
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl \
    && rm -rf /var/lib/apt/lists/*

COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

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
