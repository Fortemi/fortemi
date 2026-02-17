"""GLiNER NER sidecar service for Fortemi extraction pipeline.

Exposes a FastAPI endpoint for zero-shot named entity recognition using
GLiNER (Zaratiana et al., NAACL 2024). Runs on CPU — does not compete
with Ollama for GPU resources.

Environment variables:
    GLINER_MODEL: HuggingFace model ID (default: urchade/gliner_large-v2.1)
    GLINER_PORT: Server port (default: 8090)
    GLINER_THRESHOLD: Minimum confidence score (default: 0.3)
    GLINER_MAX_LENGTH: Maximum text length in characters (default: 10000)
"""

import os
import logging
from contextlib import asynccontextmanager

from fastapi import FastAPI
from pydantic import BaseModel, Field

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger("gliner-sidecar")

MODEL_NAME = os.environ.get("GLINER_MODEL", "urchade/gliner_large-v2.1")
DEFAULT_THRESHOLD = float(os.environ.get("GLINER_THRESHOLD", "0.3"))
MAX_LENGTH = int(os.environ.get("GLINER_MAX_LENGTH", "10000"))

# GLiNER model instance (loaded once at startup)
model = None


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Load GLiNER model on startup."""
    global model
    logger.info(f"Loading GLiNER model: {MODEL_NAME}")
    from gliner import GLiNER
    model = GLiNER.from_pretrained(MODEL_NAME)
    logger.info(f"GLiNER model loaded: {MODEL_NAME}")
    yield
    logger.info("GLiNER sidecar shutting down")


app = FastAPI(title="GLiNER NER Sidecar", lifespan=lifespan)


class ExtractRequest(BaseModel):
    text: str = Field(..., description="Text to extract entities from")
    entity_types: list[str] = Field(
        ..., description="Entity type labels for zero-shot NER"
    )
    threshold: float = Field(
        default=DEFAULT_THRESHOLD,
        ge=0.0,
        le=1.0,
        description="Minimum confidence score",
    )


class Entity(BaseModel):
    text: str = Field(..., description="Extracted entity text span")
    label: str = Field(..., description="Entity type label")
    score: float = Field(..., description="Confidence score")
    start: int = Field(..., description="Character start offset")
    end: int = Field(..., description="Character end offset")


class ExtractResponse(BaseModel):
    entities: list[Entity]
    model: str
    text_length: int


@app.post("/extract", response_model=ExtractResponse)
async def extract_entities(req: ExtractRequest):
    """Extract named entities using GLiNER zero-shot NER."""
    # Truncate text to max length (GLiNER has a 512-token limit internally,
    # but handles longer text via sliding window)
    text = req.text[:MAX_LENGTH]

    # GLiNER predict_entities returns list of dicts
    raw_entities = model.predict_entities(
        text, req.entity_types, threshold=req.threshold
    )

    # Deduplicate: keep highest-scoring span for overlapping entities
    seen = set()
    entities = []
    for ent in raw_entities:
        key = (ent["text"].lower(), ent["label"])
        if key not in seen:
            seen.add(key)
            entities.append(
                Entity(
                    text=ent["text"],
                    label=ent["label"],
                    score=round(ent["score"], 4),
                    start=ent.get("start", 0),
                    end=ent.get("end", 0),
                )
            )

    return ExtractResponse(
        entities=entities,
        model=MODEL_NAME,
        text_length=len(text),
    )


@app.get("/health")
async def health():
    """Health check endpoint."""
    return {
        "status": "healthy" if model is not None else "loading",
        "model": MODEL_NAME,
    }


if __name__ == "__main__":
    import uvicorn

    port = int(os.environ.get("GLINER_PORT", "8090"))
    uvicorn.run(app, host="0.0.0.0", port=port)
