# Test Data Manifest

Detailed specifications for each test file in the UAT data package.

## Images

### jpeg-with-exif.jpg

**Category**: Image with full EXIF metadata

**Specifications**:
- Format: JPEG
- Size: ~300-500KB
- Dimensions: 4032 x 3024 pixels (12MP, typical smartphone)
- EXIF data:
  - **GPS**: 48.8584°N, 2.2945°E (Eiffel Tower, Paris)
  - **Altitude**: 35 meters above sea level
  - **DateTime**: 2024-06-15 14:30:00 UTC
  - **Make**: Apple
  - **Model**: iPhone 15 Pro
  - **Orientation**: 1 (normal)
  - **Software**: iOS 17.5

**Content**: Landscape photo with recognizable landmarks

**Expected Extraction**:
```json
{
  "exif": {
    "datetime": "2024-06-15T14:30:00Z",
    "gps": {
      "latitude": 48.8584,
      "longitude": 2.2945,
      "altitude": 35.0
    },
    "device": {
      "make": "Apple",
      "model": "iPhone 15 Pro",
      "software": "iOS 17.5"
    },
    "dimensions": [4032, 3024],
    "orientation": 1
  },
  "provenance": {
    "location_geography": "ST_SetSRID(ST_MakePoint(2.2945, 48.8584), 4326)::geography",
    "created_at_utc": "2024-06-15T14:30:00Z"
  }
}
```

**Test Scenarios**:
- GPS coordinate extraction and PostGIS conversion
- Datetime parsing with timezone handling
- Camera device metadata extraction
- W3C PROV spatial provenance tracking

**Generation**:
```bash
python3 scripts/create-exif-images.py --preset paris-eiffel
```

---

### jpeg-no-metadata.jpg

**Category**: Image without EXIF metadata

**Specifications**:
- Format: JPEG
- Size: ~200KB
- Dimensions: 1920 x 1080 pixels
- EXIF data: None (stripped)

**Content**: Simple landscape or abstract pattern

**Expected Extraction**:
```json
{
  "exif": null,
  "provenance": null
}
```

**Test Scenarios**:
- Graceful handling of missing EXIF
- Image still accepted and stored
- No provenance data created

**Generation**:
```bash
python3 scripts/create-exif-images.py --strip-metadata landscape.jpg
```

---

### png-transparent.png

**Category**: PNG with transparency (no EXIF support)

**Specifications**:
- Format: PNG
- Size: ~50KB
- Dimensions: 512 x 512 pixels
- Transparency: Alpha channel present
- EXIF data: N/A (PNG doesn't support EXIF)

**Content**: Icon or logo with transparent background

**Expected Extraction**:
```json
{
  "format": "PNG",
  "dimensions": [512, 512],
  "has_transparency": true
}
```

**Test Scenarios**:
- PNG format support
- Transparency preservation
- No EXIF attempted (format limitation)

**Generation**:
```bash
convert -size 512x512 xc:none -fill blue -draw "circle 256,256 256,128" png-transparent.png
```

---

### webp-modern.webp

**Category**: Modern WebP format

**Specifications**:
- Format: WebP
- Size: ~100KB
- Dimensions: 1920 x 1080 pixels
- Compression: Lossy, quality 85

**Content**: General photo or scene

**Expected Extraction**:
```json
{
  "format": "WebP",
  "dimensions": [1920, 1080]
}
```

**Test Scenarios**:
- WebP format support
- Modern image format handling

**Generation**:
```bash
convert sample.jpg -quality 85 webp-modern.webp
```

---

### faces-group-photo.jpg

**Category**: Image with human faces for vision extraction

**Specifications**:
- Format: JPEG
- Size: ~400KB
- Dimensions: 2048 x 1536 pixels
- Content: Group photo with 3-5 people
- Faces: Clearly visible, various poses

**Expected Vision Extraction**:
```json
{
  "vision_description": "A group photo of five people standing outdoors in front of a building. Three women and two men, smiling and posing together. Background shows trees and a modern glass facade.",
  "detected_objects": ["person", "person", "person", "person", "person", "building", "tree"],
  "face_count": 5
}
```

**Test Scenarios**:
- AI vision model inference
- Face detection
- Scene understanding

**Source**: Download from Unsplash or generate with Stable Diffusion
```bash
# Example with Unsplash
wget "https://source.unsplash.com/2048x1536/?group,people" -O faces-group-photo.jpg
```

---

### object-scene.jpg

**Category**: Image with recognizable objects

**Specifications**:
- Format: JPEG
- Size: ~350KB
- Dimensions: 1920 x 1080 pixels
- Content: Indoor scene with common objects (laptop, coffee cup, plant, etc.)

**Expected Vision Extraction**:
```json
{
  "vision_description": "A desk workspace with a laptop computer, coffee mug, potted plant, and notebook. Natural lighting from a window on the left.",
  "detected_objects": ["laptop", "cup", "plant", "notebook", "desk", "window"]
}
```

**Test Scenarios**:
- Object detection
- Scene understanding
- Contextual description generation

**Source**: Download from Unsplash
```bash
wget "https://source.unsplash.com/1920x1080/?workspace,desk" -O object-scene.jpg
```

---

### emoji-unicode-名前.jpg

**Category**: Unicode filename edge case

**Specifications**:
- Format: JPEG
- Size: ~200KB
- Dimensions: 1024 x 768 pixels
- Filename: Contains emoji (🎨) and Japanese characters (名前 = "name")

**Content**: Simple photo or pattern

**Test Scenarios**:
- Unicode filename handling
- Emoji in filenames
- Japanese character support
- No mojibake (character corruption)

**Generation**:
```bash
cp sample.jpg "emoji-unicode-名前.jpg"
```

---

## Documents

### pdf-single-page.pdf

**Category**: Simple PDF document

**Specifications**:
- Format: PDF 1.4
- Size: ~50KB
- Pages: 1
- Content: Plain text with heading and paragraphs
- Fonts: Embedded
- Images: None

**Text Content**:
```
Test Document: Single Page PDF

This is a test document for validating PDF text extraction in matric-memory.

It contains multiple paragraphs to ensure proper text flow extraction.

Key points:
- Simple structure
- No complex formatting
- Plain text only
```

**Expected Extraction**:
```json
{
  "document_type": "pdf",
  "chunking_strategy": "per_section",
  "page_count": 1,
  "extracted_text": "Test Document: Single Page PDF\n\nThis is a test document...",
  "chunks": [
    {
      "section": "full_document",
      "content": "...",
      "char_count": 250
    }
  ]
}
```

**Test Scenarios**:
- PDF text extraction
- Single-page handling
- Document type detection from extension

**Generation**:
```bash
# Using LibreOffice headless
echo -e "Test Document: Single Page PDF\n\nThis is a test document..." > temp.txt
libreoffice --headless --convert-to pdf temp.txt --outdir documents/
mv temp.pdf documents/pdf-single-page.pdf
```

---

### code-python.py

**Category**: Python source code

**Specifications**:
- Language: Python 3.11+
- Size: ~5KB
- Lines: ~150 lines
- Features: Functions, classes, docstrings, type hints

**Content**:
```python
"""Sample Python module for testing code chunking."""

from typing import List, Optional
import json


class DataProcessor:
    """Processes data with various transformations."""

    def __init__(self, config: dict):
        self.config = config

    def process(self, data: List[dict]) -> List[dict]:
        """Process a list of data items."""
        return [self._transform(item) for item in data]

    def _transform(self, item: dict) -> dict:
        """Transform a single item."""
        # Implementation here
        return item


def main():
    processor = DataProcessor({"mode": "strict"})
    result = processor.process([{"id": 1}, {"id": 2}])
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
```

**Expected Extraction**:
```json
{
  "document_type": "python",
  "chunking_strategy": "syntactic",
  "tree_sitter_language": "python",
  "chunks": [
    {"type": "import_statement", "content": "from typing import List...", "line": 3},
    {"type": "class_definition", "name": "DataProcessor", "line": 7},
    {"type": "function_definition", "name": "main", "line": 22}
  ]
}
```

**Test Scenarios**:
- Python syntax detection
- Tree-sitter syntactic chunking
- Code structure preservation

---

### code-rust.rs

**Category**: Rust source code

**Specifications**:
- Language: Rust 2021 edition
- Size: ~4KB
- Lines: ~100 lines
- Features: Structs, impl blocks, functions, traits

**Content**:
```rust
//! Sample Rust module for testing code chunking.

use std::collections::HashMap;

/// Configuration for the processor
#[derive(Debug, Clone)]
pub struct ProcessorConfig {
    pub mode: String,
    pub threshold: f64,
}

/// Main data processor
pub struct DataProcessor {
    config: ProcessorConfig,
    cache: HashMap<String, String>,
}

impl DataProcessor {
    /// Create a new processor with given config
    pub fn new(config: ProcessorConfig) -> Self {
        Self {
            config,
            cache: HashMap::new(),
        }
    }

    /// Process input data
    pub fn process(&mut self, data: &str) -> String {
        if let Some(cached) = self.cache.get(data) {
            return cached.clone();
        }

        let result = self.transform(data);
        self.cache.insert(data.to_string(), result.clone());
        result
    }

    fn transform(&self, data: &str) -> String {
        // Implementation
        data.to_uppercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_processor() {
        let config = ProcessorConfig {
            mode: "strict".to_string(),
            threshold: 0.5,
        };
        let mut processor = DataProcessor::new(config);
        assert_eq!(processor.process("test"), "TEST");
    }
}
```

**Expected Extraction**:
```json
{
  "document_type": "rust",
  "chunking_strategy": "syntactic",
  "tree_sitter_language": "rust",
  "chunks": [
    {"type": "use_declaration", "line": 3},
    {"type": "struct_item", "name": "ProcessorConfig", "line": 6},
    {"type": "struct_item", "name": "DataProcessor", "line": 13},
    {"type": "impl_item", "target": "DataProcessor", "line": 19},
    {"type": "mod_item", "name": "tests", "line": 44}
  ]
}
```

**Test Scenarios**:
- Rust syntax detection
- Module-level chunking
- Trait and impl block separation

---

### code-javascript.js

**Category**: JavaScript (ES6+)

**Specifications**:
- Language: JavaScript ES2022
- Size: ~3KB
- Features: Classes, arrow functions, async/await, modules

**Content**:
```javascript
/**
 * Sample JavaScript module for testing code chunking
 * @module DataProcessor
 */

import fs from 'fs/promises';

/**
 * Data processor class
 */
export class DataProcessor {
  constructor(config) {
    this.config = config;
    this.cache = new Map();
  }

  /**
   * Process data asynchronously
   * @param {Array} data - Input data
   * @returns {Promise<Array>} Processed data
   */
  async process(data) {
    const results = await Promise.all(
      data.map(item => this.transform(item))
    );
    return results;
  }

  async transform(item) {
    const cached = this.cache.get(item.id);
    if (cached) return cached;

    const result = {
      ...item,
      processed: true,
      timestamp: Date.now()
    };

    this.cache.set(item.id, result);
    return result;
  }
}

/**
 * Utility function
 */
export const loadConfig = async (path) => {
  const content = await fs.readFile(path, 'utf-8');
  return JSON.parse(content);
};

// Default export
export default DataProcessor;
```

**Expected Extraction**:
```json
{
  "document_type": "javascript",
  "chunking_strategy": "syntactic",
  "tree_sitter_language": "javascript",
  "chunks": [
    {"type": "import_statement", "line": 6},
    {"type": "class_declaration", "name": "DataProcessor", "line": 11},
    {"type": "function_declaration", "name": "loadConfig", "line": 46},
    {"type": "export_statement", "line": 52}
  ]
}
```

---

### code-typescript.ts

**Category**: TypeScript

**Specifications**:
- Language: TypeScript 5.0+
- Size: ~4KB
- Features: Interfaces, generics, type annotations, decorators

**Content**:
```typescript
/**
 * Sample TypeScript module for testing code chunking
 */

interface ProcessorConfig {
  mode: 'strict' | 'lenient';
  threshold: number;
}

interface DataItem {
  id: string;
  value: unknown;
  timestamp?: Date;
}

/**
 * Generic data processor
 */
export class DataProcessor<T extends DataItem> {
  private config: ProcessorConfig;
  private cache: Map<string, T>;

  constructor(config: ProcessorConfig) {
    this.config = config;
    this.cache = new Map();
  }

  /**
   * Process items with type safety
   */
  async process(items: T[]): Promise<T[]> {
    return Promise.all(items.map(item => this.transform(item)));
  }

  private async transform(item: T): Promise<T> {
    const cached = this.cache.get(item.id);
    if (cached) return cached;

    const result = {
      ...item,
      timestamp: new Date()
    } as T;

    this.cache.set(item.id, result);
    return result;
  }

  clearCache(): void {
    this.cache.clear();
  }
}

/**
 * Type-safe config loader
 */
export async function loadConfig(path: string): Promise<ProcessorConfig> {
  const fs = await import('fs/promises');
  const content = await fs.readFile(path, 'utf-8');
  return JSON.parse(content) as ProcessorConfig;
}
```

**Expected Extraction**:
```json
{
  "document_type": "typescript",
  "chunking_strategy": "syntactic",
  "tree_sitter_language": "typescript",
  "chunks": [
    {"type": "interface_declaration", "name": "ProcessorConfig", "line": 5},
    {"type": "interface_declaration", "name": "DataItem", "line": 10},
    {"type": "class_declaration", "name": "DataProcessor", "line": 19},
    {"type": "function_declaration", "name": "loadConfig", "line": 55}
  ]
}
```

---

### markdown-formatted.md

**Category**: Markdown with various formatting

**Specifications**:
- Format: Markdown (CommonMark)
- Size: ~8KB
- Features: Headers, lists, code blocks, tables, links, images

**Content**:
```markdown
# Test Document: Markdown Formatting

This document tests markdown chunking with semantic paragraph splitting.

## Introduction

Markdown is a lightweight markup language for creating formatted text. It supports:

- **Bold** and *italic* text
- `Inline code` snippets
- [Links](https://example.com)
- ![Images](image.jpg)

## Code Examples

Here's a Python code block:

\`\`\`python
def hello_world():
    print("Hello, World!")
\`\`\`

And a JavaScript example:

\`\`\`javascript
const greet = () => console.log("Hello!");
\`\`\`

## Tables

| Column 1 | Column 2 | Column 3 |
|----------|----------|----------|
| Data 1   | Data 2   | Data 3   |
| Data 4   | Data 5   | Data 6   |

## Lists

1. First item
2. Second item
   - Nested item A
   - Nested item B
3. Third item

## Conclusion

This document covers various markdown elements for comprehensive testing.
```

**Expected Extraction**:
```json
{
  "document_type": "markdown",
  "chunking_strategy": "semantic",
  "chunks": [
    {"section": "heading_1", "content": "Test Document: Markdown Formatting", "level": 1},
    {"section": "paragraph", "content": "This document tests...", "parent": "Introduction"},
    {"section": "heading_2", "content": "Introduction", "level": 2},
    {"section": "list", "content": "- **Bold** and *italic*...", "parent": "Introduction"},
    {"section": "heading_2", "content": "Code Examples", "level": 2},
    {"section": "code_block", "language": "python", "parent": "Code Examples"},
    {"section": "code_block", "language": "javascript", "parent": "Code Examples"},
    {"section": "heading_2", "content": "Tables", "level": 2},
    {"section": "table", "rows": 3, "cols": 3, "parent": "Tables"}
  ]
}
```

**Test Scenarios**:
- Markdown parsing
- Semantic chunking by paragraphs
- Code block extraction
- Table handling

---

### json-config.json

**Category**: JSON configuration file

**Specifications**:
- Format: JSON
- Size: ~2KB
- Structure: Nested objects and arrays
- Valid: Yes (no syntax errors)

**Content**:
```json
{
  "name": "matric-memory-test-config",
  "version": "1.0.0",
  "database": {
    "host": "localhost",
    "port": 5432,
    "name": "matric_test",
    "pool": {
      "min_connections": 2,
      "max_connections": 10,
      "timeout_seconds": 30
    }
  },
  "embedding": {
    "model": "nomic-embed-text-v1.5",
    "dimensions": 768,
    "truncate_dim": 256,
    "batch_size": 100
  },
  "search": {
    "fts_script_detection": true,
    "fts_trigram_fallback": true,
    "fts_bigram_cjk": true,
    "default_limit": 50,
    "max_limit": 500
  },
  "features": [
    "semantic_search",
    "provenance_tracking",
    "multilingual_fts",
    "document_chunking"
  ],
  "logging": {
    "level": "info",
    "format": "json",
    "outputs": ["stdout", "file"]
  }
}
```

**Expected Extraction**:
```json
{
  "document_type": "json",
  "chunking_strategy": "whole",
  "is_valid_json": true,
  "extracted_structure": {
    "top_level_keys": ["name", "version", "database", "embedding", "search", "features", "logging"],
    "depth": 3
  }
}
```

**Test Scenarios**:
- JSON parsing and validation
- Structured data extraction
- Whole-document chunking for config files

---

### yaml-config.yaml

**Category**: YAML configuration file

**Specifications**:
- Format: YAML 1.2
- Size: ~2KB
- Structure: Nested maps and sequences
- Valid: Yes

**Content**:
```yaml
name: matric-memory-test-config
version: 1.0.0

database:
  host: localhost
  port: 5432
  name: matric_test
  pool:
    min_connections: 2
    max_connections: 10
    timeout_seconds: 30

embedding:
  model: nomic-embed-text-v1.5
  dimensions: 768
  truncate_dim: 256
  batch_size: 100

search:
  fts_script_detection: true
  fts_trigram_fallback: true
  fts_bigram_cjk: true
  default_limit: 50
  max_limit: 500

features:
  - semantic_search
  - provenance_tracking
  - multilingual_fts
  - document_chunking

logging:
  level: info
  format: json
  outputs:
    - stdout
    - file
```

**Expected Extraction**:
```json
{
  "document_type": "yaml",
  "chunking_strategy": "whole",
  "is_valid_yaml": true,
  "extracted_structure": {
    "top_level_keys": ["name", "version", "database", "embedding", "search", "features", "logging"]
  }
}
```

---

### csv-data.csv

**Category**: CSV data file

**Specifications**:
- Format: CSV (RFC 4180)
- Size: ~5KB
- Rows: ~100 rows (including header)
- Columns: 5

**Content**:
```csv
id,name,email,created_at,status
1,Alice Johnson,alice@example.com,2024-01-15T10:30:00Z,active
2,Bob Smith,bob@example.com,2024-01-16T11:45:00Z,active
3,Charlie Davis,charlie@example.com,2024-01-17T09:15:00Z,inactive
4,Diana Prince,diana@example.com,2024-01-18T14:20:00Z,active
...
```

**Expected Extraction**:
```json
{
  "document_type": "csv",
  "chunking_strategy": "whole",
  "csv_metadata": {
    "columns": ["id", "name", "email", "created_at", "status"],
    "row_count": 100,
    "has_header": true
  }
}
```

**Test Scenarios**:
- CSV parsing
- Structured data handling
- Tabular data extraction

**Generation**:
```python
import csv
import random
from datetime import datetime, timedelta

with open('documents/csv-data.csv', 'w', newline='') as f:
    writer = csv.writer(f)
    writer.writerow(['id', 'name', 'email', 'created_at', 'status'])

    names = ['Alice Johnson', 'Bob Smith', 'Charlie Davis', 'Diana Prince', ...]
    for i in range(1, 101):
        name = random.choice(names)
        email = f"{name.split()[0].lower()}@example.com"
        created = datetime(2024, 1, 1) + timedelta(days=i)
        status = random.choice(['active', 'inactive'])
        writer.writerow([i, name, email, created.isoformat() + 'Z', status])
```

---

### config.txt

**Category**: Plain text configuration

**Specifications**:
- Format: Plain text
- Size: ~500 bytes
- Content: Simple key=value configuration

**Test Scenarios**:
- Plain text type detection
- Config file handling

---

### readme.txt

**Category**: Plain text README

**Specifications**:
- Format: Plain text
- Size: ~1KB
- Content: Project readme in plain text format

**Test Scenarios**:
- Document type detection (DOC-005, DOC-011)
- Text processing

---

### test.txt

**Category**: Plain text test document

**Specifications**:
- Format: Plain text
- Size: ~500 bytes
- Content: Simple test content

**Test Scenarios**:
- Basic text handling
- Document type detection (DOC-006)

---

### test-document.pdf

**Category**: Additional PDF test document

**Specifications**:
- Format: PDF
- Size: ~50KB
- Content: Generated test PDF for supplementary testing

**Test Scenarios**:
- PDF type detection
- Alternate PDF handling

---

## Audio

### english-speech-5s.mp3

**Category**: English speech sample

**Specifications**:
- Format: MP3
- Duration: 5 seconds
- Bitrate: 128 kbps
- Sample rate: 44.1 kHz
- Language: English (US)
- Content: Clear speech, no background noise

**Transcript**:
```
"Welcome to Matric Memory. This is a test of the audio transcription system."
```

**Expected Extraction**:
```json
{
  "document_type": "audio",
  "extraction_strategy": "whisper",
  "transcription": {
    "text": "Welcome to Matric Memory. This is a test of the audio transcription system.",
    "language": "en",
    "duration": 5.0,
    "confidence": 0.95
  }
}
```

**Test Scenarios**:
- Audio transcription with Whisper
- English language detection
- Speech-to-text accuracy

**Generation**:
```bash
# Using gTTS (Google Text-to-Speech)
python3 -c "
from gtts import gTTS
text = 'Welcome to Matric Memory. This is a test of the audio transcription system.'
tts = gTTS(text=text, lang='en', slow=False)
tts.save('english-speech-5s.mp3')
"
```

---

### spanish-greeting.mp3

**Category**: Spanish speech sample

**Specifications**:
- Format: MP3
- Duration: 3-4 seconds
- Language: Spanish (Spain)
- Content: Simple greeting

**Transcript**:
```
"Hola, bienvenido a Matric Memory."
```

**Expected Extraction**:
```json
{
  "document_type": "audio",
  "extraction_strategy": "whisper",
  "transcription": {
    "text": "Hola, bienvenido a Matric Memory.",
    "language": "es",
    "duration": 3.5
  }
}
```

**Test Scenarios**:
- Multilingual transcription
- Spanish language detection

**Generation**:
```python
from gtts import gTTS
text = "Hola, bienvenido a Matric Memory."
tts = gTTS(text=text, lang='es', slow=False)
tts.save('audio/spanish-greeting.mp3')
```

---

### chinese-phrase.mp3

**Category**: Chinese (Mandarin) speech sample

**Specifications**:
- Format: MP3
- Duration: 3-4 seconds
- Language: Chinese (Mandarin)
- Content: Simple phrase

**Transcript**:
```
"欢迎使用 Matric Memory"
(Huānyíng shǐyòng Matric Memory - Welcome to use Matric Memory)
```

**Expected Extraction**:
```json
{
  "document_type": "audio",
  "extraction_strategy": "whisper",
  "transcription": {
    "text": "欢迎使用 Matric Memory",
    "language": "zh",
    "duration": 3.0
  }
}
```

**Test Scenarios**:
- CJK language transcription
- Chinese character output

**Generation**:
```python
from gtts import gTTS
text = "欢迎使用 Matric Memory"
tts = gTTS(text=text, lang='zh-CN', slow=False)
tts.save('audio/chinese-phrase.mp3')
```

---

## Multilingual Text

### english.txt

**Content** (200-300 words):
```
The quick brown fox jumps over the lazy dog. This sentence contains every letter of the English alphabet at least once.

Natural language processing enables computers to understand, interpret, and generate human language. Modern NLP systems use transformer architectures and attention mechanisms to achieve state-of-the-art results on tasks like translation, summarization, and question answering.

Full-text search with stemming allows users to find documents even when they search for different word forms. For example, searching for "run" should also match "running", "runs", and "ran". PostgreSQL's to_tsquery function handles this automatically for English text.

Testing edge cases is crucial for robust software. Consider boundary values, empty inputs, null pointers, and unicode characters. Comprehensive test coverage catches bugs early in the development cycle.
```

**Test Query**: "running" should match "run", "runs", "ran" (stemming)

---

### german.txt

**Content** (200-300 words):
```
Die deutsche Sprache gehört zur westgermanischen Sprachgruppe und wird von über 100 Millionen Menschen gesprochen.

Volltext-Suche mit Wortstammerkennung ermöglicht es Benutzern, Dokumente zu finden, auch wenn sie nach verschiedenen Wortformen suchen. Zum Beispiel sollte die Suche nach "laufen" auch "läuft", "lief" und "gelaufen" finden. PostgreSQL unterstützt deutsche Wortstammerkennung durch die entsprechende Sprachkonfiguration.

Natürliche Sprachverarbeitung (NLP) hat in den letzten Jahren enorme Fortschritte gemacht. Moderne Systeme können Texte übersetzen, zusammenfassen und Fragen beantworten. Die Transformer-Architektur hat dabei eine Schlüsselrolle gespielt.

Umlaute wie ä, ö und ü sind wichtige Bestandteile der deutschen Schrift. Das ß (Eszett) wird in Deutschland verwendet, während in der Schweiz ss geschrieben wird.
```

**Test Query**: "laufen" should match "läuft", "lief", "gelaufen" (German stemming)

---

### french.txt

**Content**:
```
Le français est une langue romane parlée par environ 300 millions de personnes dans le monde.

La recherche en texte intégral avec normalisation permet aux utilisateurs de trouver des documents même lorsqu'ils recherchent différentes formes de mots. Par exemple, la recherche de "courir" devrait également correspondre à "cours", "courons" et "couru". PostgreSQL prend en charge la normalisation française via sa configuration linguistique.

Le traitement du langage naturel (NLP) a connu des progrès remarquables ces dernières années. Les systèmes modernes peuvent traduire, résumer et répondre aux questions. L'architecture Transformer a joué un rôle clé dans ces avancées.

Les accents français incluent l'aigu (é), le grave (è), le circonflexe (ê) et la cédille (ç). Ces signes diacritiques sont essentiels pour la prononciation et le sens correct.
```

**Test Query**: "courir" should match "cours", "courons", "couru" (French stemming)

---

### spanish.txt

**Content**:
```
El español es una lengua romance hablada por más de 500 millones de personas en todo el mundo.

La búsqueda de texto completo con lematización permite a los usuarios encontrar documentos incluso cuando buscan diferentes formas de palabras. Por ejemplo, buscar "correr" también debería encontrar "corre", "corriendo" y "corrió". PostgreSQL admite la lematización española a través de su configuración de idioma.

El procesamiento del lenguaje natural (PLN) ha experimentado avances notables en los últimos años. Los sistemas modernos pueden traducir, resumir y responder preguntas. La arquitectura Transformer ha desempeñado un papel clave en estos avances.

Los acentos españoles incluyen la tilde (á, é, í, ó, ú) y la diéresis (ü). La letra ñ es característica única del español. Los signos de interrogación (¿?) y exclamación (¡!) se usan al principio y al final de las oraciones.
```

**Test Query**: "correr" should match "corre", "corriendo", "corrió" (Spanish stemming)

---

### portuguese.txt

**Content**:
```
O português é uma língua românica falada por mais de 250 milhões de pessoas em todo o mundo.

A pesquisa de texto completo com lematização permite que os usuários encontrem documentos mesmo quando pesquisam diferentes formas de palavras. Por exemplo, pesquisar "correr" também deve encontrar "corre", "correndo" e "correu". PostgreSQL suporta lematização portuguesa através de sua configuração de idioma.

O processamento de linguagem natural (PLN) experimentou avanços notáveis nos últimos anos. Sistemas modernos podem traduzir, resumir e responder perguntas. A arquitetura Transformer desempenhou um papel fundamental nesses avanços.

Os acentos portugueses incluem agudo (á, é), circunflexo (â, ê, ô), til (ã, õ) e crase (à). A cedilha (ç) também é usada. Existem diferenças entre o português europeu e o brasileiro.
```

**Test Query**: "correr" should match "corre", "correndo", "correu" (Portuguese stemming)

---

### russian.txt

**Content** (Cyrillic):
```
Русский язык является восточнославянским языком и используется более чем 250 миллионами человек по всему миру.

Полнотекстовый поиск с основами слов позволяет пользователям находить документы, даже если они ищут разные формы слов. Например, поиск "бежать" должен также находить "бежит", "бегут" и "бежал". PostgreSQL поддерживает русское словообразование через соответствующую языковую конфигурацию.

Обработка естественного языка (NLP) достигла замечательных успехов в последние годы. Современные системы могут переводить, резюмировать и отвечать на вопросы. Архитектура трансформера сыграла ключевую роль в этих достижениях.

Кириллица используется для написания русского языка. Буквы включают а, б, в, г, д, е, ё, ж, з, и, й, к, л, м, н, о, п, р, с, т, у, ф, х, ц, ч, ш, щ, ъ, ы, ь, э, ю, я.
```

**Test Query**: "бежать" should match "бежит", "бегут", "бежал" (Russian stemming)

---

### chinese-simplified.txt

**Content** (Simplified Chinese):
```
中文是世界上使用人数最多的语言之一,有超过十亿人使用。

全文搜索对于中日韩(CJK)语言使用字符二元组匹配,因为这些语言不使用空格分隔单词。PostgreSQL通过pg_bigm扩展支持CJK文本的高效搜索。

自然语言处理(NLP)技术在近年来取得了显著进展。现代系统可以翻译、摘要和回答问题。Transformer架构在这些进展中发挥了关键作用。

中文文本包含常用汉字、标点符号和阿拉伯数字。简体中文在中国大陆使用,而繁体中文在台湾和香港使用。搜索"北京"应该能找到包含"北京市"、"北京大学"的文档。
```

**Test Query**: "北京" should use bigram matching for "北京市", "北京大学"

---

### japanese.txt

**Content** (Japanese - Hiragana, Katakana, Kanji):
```
日本語は日本で話されている言語で、約1億2500万人が使用しています。

全文検索はCJK言語に対してバイグラム(2文字組み合わせ)マッチングを使用します。これらの言語は単語を空白で区切らないため、PostgreSQLのpg_bigm拡張機能を使用して効率的な検索を実現します。

自然言語処理(NLP)技術は近年著しい進歩を遂げています。最新のシステムは翻訳、要約、質問応答が可能です。Transformerアーキテクチャがこれらの進歩において重要な役割を果たしました。

日本語のテキストには、ひらがな、カタカナ、漢字が含まれます。「東京」を検索すると「東京都」や「東京大学」を含む文書が見つかるはずです。
```

**Test Query**: "東京" should use bigram matching for "東京都", "東京大学"

---

### korean.txt

**Content** (Korean - Hangul):
```
한국어는 한국과 북한에서 사용되는 언어로 약 7700만 명이 사용합니다.

전체 텍스트 검색은 CJK 언어에 대해 바이그램(2글자 조합) 매칭을 사용합니다. 이러한 언어는 공백으로 단어를 구분하지 않기 때문에 PostgreSQL의 pg_bigm 확장을 사용하여 효율적인 검색을 구현합니다.

자연어 처리(NLP) 기술은 최근 몇 년간 현저한 발전을 이루었습니다. 최신 시스템은 번역, 요약, 질문 응답이 가능합니다. Transformer 아키텍처가 이러한 발전에 핵심적인 역할을 했습니다.

한국어 텍스트는 한글로 구성됩니다. "서울"을 검색하면 "서울시"나 "서울대학교"가 포함된 문서를 찾을 수 있어야 합니다.
```

**Test Query**: "서울" should use bigram matching for "서울시", "서울대학교"

---

### arabic.txt

**Content** (Arabic - RTL):
```
اللغة العربية هي إحدى أكثر اللغات انتشارًا في العالم، حيث يتحدث بها أكثر من 400 مليون شخص.

يستخدم البحث النصي الكامل للغات التي تُكتب من اليمين إلى اليسار مثل العربية الترميز الصحيح. يدعم PostgreSQL النصوص العربية من خلال تكوين اللغة المناسب.

شهدت معالجة اللغة الطبيعية تقدمًا ملحوظًا في السنوات الأخيرة. يمكن للأنظمة الحديثة الترجمة والتلخيص والإجابة على الأسئلة. لعبت بنية المحول دورًا رئيسيًا في هذه التطورات.

النص العربي يتضمن علامات التشكيل مثل الفتحة والكسرة والضمة. اللغة العربية تُكتب من اليمين إلى اليسار وتحتوي على 28 حرفًا.
```

**Test Query**: Basic tokenization (no stemming in current implementation)

---

### greek.txt

**Content** (Greek):
```
Η ελληνική γλώσσα είναι μία από τις αρχαιότερες γλώσσες στον κόσμο και ομιλείται από περίπου 13 εκατομμύρια ανθρώπους.

Η αναζήτηση πλήρους κειμένου για την ελληνική χρησιμοποιεί βασική τμηματοποίηση. Το PostgreSQL υποστηρίζει ελληνικό κείμενο μέσω της κατάλληλης γλωσσικής διαμόρφωσης.

Η επεξεργασία φυσικής γλώσσας έχει σημειώσει αξιοσημείωτη πρόοδο τα τελευταία χρόνια. Τα σύγχρονα συστήματα μπορούν να μεταφράζουν, να συνοψίζουν και να απαντούν σε ερωτήσεις.

Το ελληνικό αλφάβητο περιλαμβάνει γράμματα όπως α, β, γ, δ, ε, ζ, η, θ, ι, κ, λ, μ, ν, ξ, ο, π, ρ, σ, τ, υ, φ, χ, ψ, ω.
```

**Test Query**: Basic tokenization

---

### hebrew.txt

**Content** (Hebrew - RTL):
```
העברית היא שפה שמית המדוברת על ידי כ-9 מיליון אנשים ברחבי העולם.

חיפוש טקסט מלא לשפות הנכתבות מימין לשמאל כמו עברית משתמש בקידוד נכון. PostgreSQL תומך בטקסט עברי באמצעות תצורת השפה המתאימה.

עיבוד שפה טבעית חווה התקדמות ניכרת בשנים האחרונות. מערכות מודרניות יכולות לתרגם, לסכם ולענות על שאלות. ארכיטקטורת הטרנספורמר שיחקה תפקיד מרכזי בהתקדמות זו.

הטקסט העברי כולל ניקוד אך בדרך כלל נכתב בלי אותו. האלפבית העברי מכיל 22 אותיות.
```

**Test Query**: Basic tokenization (RTL support)

---

### emoji-heavy.txt

**Content**:
```
🎉 Welcome to Matric Memory! 🚀

Full-text search supports emoji through trigram indexing. 🔍✨

Common emoji usage:
- 😀😁😂🤣 Happy faces
- 🔥💯👍 Positive reactions
- 🌟⭐✨ Stars and sparkles
- 🎯🎨🎭 Activities
- 🌍🌎🌏 World globes
- 💻📱⌨️ Technology
- 🍕🍔🍟 Food

Emoji can be searched individually: 🎉 or combined: 🚀🌟

PostgreSQL's pg_trgm extension enables substring matching for emoji characters, allowing users to search for "🎉" and find all documents containing that specific emoji. 🎊🎈
```

**Test Query**: "🎉" should find documents with that emoji (trigram matching)

---

## Edge Cases

### empty.txt

**Specifications**:
- Size: 0 bytes
- Content: None

**Expected Behavior**:
- HTTP 200 (accept the file)
- Warning in metadata: "empty_content": true
- No FTS indexing (nothing to index)
- Note created with empty content

**Test**:
```bash
curl -X POST http://localhost:3000/api/v1/notes \
  -F "content=@edge-cases/empty.txt" \
  -F "tags=test,edge-case"

# Response should include:
# "metadata": {"warnings": ["File is empty"]}
```

---

### large-text-100kb.txt

**Specifications**:
- Size: ~100KB (>100,000 bytes)
- Content: Repeated lorem ipsum text to reach size threshold
- Format: Plain text

**Expected Behavior**:
- Appropriate chunking based on document type (semantic for plain text)
- Multiple chunks created (likely 5-10 chunks depending on chunk size)
- All chunks indexed for FTS
- Memory-efficient processing (streaming if possible)

**Generation**:
```python
with open('edge-cases/large-text-100kb.txt', 'w') as f:
    lorem = "Lorem ipsum dolor sit amet, consectetur adipiscing elit..."
    while f.tell() < 100000:
        f.write(lorem + "\n\n")
```

---

### binary-wrong-ext.jpg

**Specifications**:
- Actual format: Random binary data (not an image)
- Extension: `.jpg` (misleading)
- Size: ~10KB
- Magic bytes: Random (not JPEG magic bytes FF D8 FF)

**Expected Behavior**:
- EXIF extraction fails with clear error
- HTTP 400 or similar error response
- Error message: "Invalid image format" or "Failed to read EXIF data"
- File rejected, note not created

**Generation**:
```python
import os
with open('edge-cases/binary-wrong-ext.jpg', 'wb') as f:
    f.write(os.urandom(10240))  # 10KB random bytes
```

**Test**:
```bash
curl -X POST http://localhost:3000/api/v1/notes \
  -F "content=@edge-cases/binary-wrong-ext.jpg" \
  -F "tags=test,edge-case"

# Expected: HTTP 400
# {"error": "Invalid image format: Failed to read EXIF data"}
```

---

### unicode-filename-测试.txt

**Specifications**:
- Filename: Contains Chinese characters (测试 = "test")
- Content: "This file has Unicode in its filename: 测试"
- Size: ~100 bytes

**Expected Behavior**:
- Filename stored correctly in database
- No mojibake (�� characters)
- File accessible via API with correct filename
- Content searchable normally

**Test**:
```bash
curl -X POST http://localhost:3000/api/v1/notes \
  -F "content=@edge-cases/unicode-filename-测试.txt" \
  -F "tags=test,unicode"

# Verify filename in response
curl http://localhost:3000/api/v1/notes/{note_id} | jq '.note.metadata.filename'
# Should show: "unicode-filename-测试.txt"
```

---

### whitespace-only.txt

**Specifications**:
- Content: Only whitespace (spaces, tabs, newlines)
- Size: ~500 bytes
- Example: "    \n\t\t  \n\n    \n"

**Expected Behavior**:
- Accept file (HTTP 200)
- Mark as empty content after trimming
- No FTS indexing (nothing meaningful to index)
- Metadata flag: "empty_after_trim": true

**Generation**:
```python
with open('edge-cases/whitespace-only.txt', 'w') as f:
    f.write("    \n\t\t  \n\n    \n" * 20)
```

---

### malformed-json.json

**Specifications**:
- Format: JSON (claimed)
- Content: Invalid JSON syntax
- Size: ~500 bytes

**Content**:
```json
{
  "name": "test",
  "value": 123,
  "nested": {
    "key": "value"
    "missing_comma": true
  },
  "trailing_comma": true,
}
```

**Expected Behavior**:
- JSON parsing fails
- Fallback to plain text storage
- Document type: "text" instead of "json"
- Content stored as-is (no parsing)
- Warning in metadata: "json_parse_failed": true

**Test**:
```bash
curl -X POST http://localhost:3000/api/v1/notes \
  -F "content=@edge-cases/malformed-json.json" \
  -F "tags=test,edge-case"

# Check document type
curl http://localhost:3000/api/v1/notes/{note_id} | jq '.note.document_type_name'
# Should be "text" not "json"
```

---

### malware.exe

**Category**: Suspicious file extension edge case

**Specifications**:
- Format: Executable file extension
- Size: ~100 bytes
- Content: Harmless placeholder (not actual malware)

**Test Scenarios**:
- Dangerous file extension handling
- Upload security filtering

---

### script.sh

**Category**: Script file upload edge case

**Specifications**:
- Format: Shell script
- Size: ~200 bytes
- Content: Simple bash script

**Test Scenarios**:
- Script upload handling
- Executable file type detection

---

## Provenance

### paris-eiffel-tower.jpg

**Specifications**:
- GPS: 48.8584°N, 2.2945°E (Eiffel Tower, Paris, France)
- Altitude: 35 meters
- DateTime: 2024-07-14T12:00:00Z (Bastille Day)
- Camera: Canon EOS R5
- Dimensions: 3840 x 2160

**Expected Provenance**:
```sql
SELECT
  n.id,
  n.title,
  ST_AsText(p.location_geography::geometry) as location,
  p.created_at_utc,
  p.device_info->>'make' as camera_make
FROM note n
JOIN provenance_edge p ON n.id = p.revision_id
WHERE n.title LIKE '%paris%';

-- Result:
-- location: POINT(2.2945 48.8584)
-- created_at_utc: 2024-07-14 12:00:00+00
-- camera_make: Canon
```

---

### newyork-statue-liberty.jpg

**Specifications**:
- GPS: 40.6892°N, 74.0445°W (Statue of Liberty, New York, USA)
- Altitude: 10 meters
- DateTime: 2024-07-04T16:30:00Z (Independence Day)
- Camera: Nikon Z9
- Dimensions: 4096 x 2732

**Expected Provenance**:
- Location: PostGIS geography point in New York Harbor
- Temporal: July 4, 2024

---

### tokyo-shibuya.jpg

**Specifications**:
- GPS: 35.6595°N, 139.7004°E (Shibuya Crossing, Tokyo, Japan)
- Altitude: 30 meters
- DateTime: 2024-03-21T09:00:00Z
- Camera: Sony α7R V
- Dimensions: 4320 x 2880

**Expected Provenance**:
- Location: PostGIS geography point in Tokyo
- Temporal: March 21, 2024

---

### dated-2020-01-01.jpg

**Specifications**:
- GPS: None
- DateTime: 2020-01-01T00:00:00Z (millennium edge case)
- Camera: iPhone 11
- Dimensions: 3024 x 4032 (portrait)

**Test Scenario**: Temporal provenance tracking for historical date

---

### dated-2025-12-31.jpg

**Specifications**:
- GPS: None
- DateTime: 2025-12-31T23:59:59Z (end of year edge case)
- Camera: Pixel 9 Pro
- Dimensions: 4080 x 3072

**Test Scenario**: Future date handling (if current date < 2025-12-31)

---

### duplicate-content-1.txt

**Content**:
```
This is duplicate content for testing content-based deduplication.

The hash of this content should match duplicate-content-2.txt exactly.

Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.
```

**Hash (SHA-256)**: (Calculated on upload)

---

### duplicate-content-2.txt

**Content**: (Identical to duplicate-content-1.txt)
```
This is duplicate content for testing content-based deduplication.

The hash of this content should match duplicate-content-2.txt exactly.

Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.
```

**Expected Behavior**:
```sql
SELECT hash, COUNT(*) as count, array_agg(id) as note_ids
FROM note_original
GROUP BY hash
HAVING COUNT(*) > 1;

-- Result:
-- hash: <same_hash>
-- count: 2
-- note_ids: {uuid1, uuid2}
```

**Test Scenario**: Content deduplication detection via hash matching

---

## Summary Statistics

| Category | Count | Total Size |
|----------|-------|------------|
| Images | 7 | ~2.5 MB |
| Documents | 14 | ~200 KB |
| Audio | 3 | ~300 KB |
| Multilingual | 13 | ~50 KB |
| Edge Cases | 8 | ~120 KB |
| Provenance | 7 | ~3 MB |
| **Total** | **52** | **~6.2 MB** |

All test files combined should be under 10 MB to keep the repository lean.
