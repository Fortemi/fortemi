# UAT Test Fixtures

This directory contains test data for UAT execution.

## Contents

### Text Files

- `sample-note.md` - Sample markdown note content
- `sample-template.md` - Sample template with placeholders
- `sample-code.rs` - Sample Rust code for document type detection
- `sample-config.json` - Sample JSON configuration

### Binary Files (Small)

- `sample-image.png` - 1x1 pixel PNG (67 bytes) for attachment testing
- `sample-pdf.pdf` - Minimal PDF for document testing

### Data Files

- `seed-notes.json` - JSON array of seed notes for bulk import
- `test-concepts.json` - SKOS concepts for taxonomy testing

## Usage

These fixtures are referenced by UAT phase documents. They provide consistent, repeatable test data.

## Size Guidelines

All binary files are kept under 1KB to minimize repository size while still testing functionality.
