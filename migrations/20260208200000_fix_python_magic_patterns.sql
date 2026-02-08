-- Fix #199: Add magic_patterns for Python document type so content-based detection works.
-- Without these, Python code is misidentified as MDX (due to 'import ') or AsciiDoc (due to '= ').
UPDATE document_type
SET magic_patterns = ARRAY[
    '#!/usr/bin/env python',
    '#!/usr/bin/python',
    'def __init__(self',
    'if __name__',
    'from __future__ import',
    'import asyncio',
    'import sys',
    'import os'
]
WHERE name = 'python';

-- Also add more specific magic_patterns for SQLAlchemy to avoid false positives
UPDATE document_type
SET magic_patterns = ARRAY[
    'from sqlalchemy import',
    'Base = declarative_base()',
    '__tablename__',
    'Column(',
    'relationship('
]
WHERE name = 'sqlalchemy';
