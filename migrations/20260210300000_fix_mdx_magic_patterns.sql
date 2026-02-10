-- Fix MDX magic patterns to avoid false positives (#294/#287)
-- The pattern '<' alone is too broad and matches any file with angle brackets,
-- causing Python files with `import` statements to be misidentified as MDX.
-- Replace with JSX-specific patterns that require opening + closing tags.

UPDATE document_type
SET magic_patterns = ARRAY['import ', 'export default ', '</', 'export function']
WHERE name = 'mdx' AND is_system = TRUE;
