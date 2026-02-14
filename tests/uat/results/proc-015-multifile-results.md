# UAT Test Results: PROC-015

**Test ID**: PROC-015  
**Test Name**: Multiple Files on One Note  
**Date**: 2026-02-13  
**Result**: PASS ✅

## Test Parameters
- **Note ID**: 019c591d-66aa-74b3-a72b-5eb2741c26e3
- **Tag**: uat/proc-multifile

## Execution Summary

### Step 1: Create Note
- Created note with tag `uat/proc-multifile` and revision_mode="none"
- Note ID: `019c591d-66aa-74b3-a72b-5eb2741c26e3`

### Step 2: Upload Python File
- Filename: `code-python.py`
- Content Type: `text/x-python`
- Size: 1,248 bytes
- Attachment ID: `019c591d-a1a9-7353-a702-d10025713962`
- **Extraction Strategy**: `code_ast` ✅
- **Extracted**: AST metadata with 5 declarations (1 class, 3 methods, 1 function)

### Step 3: Upload Markdown File
- Filename: `markdown-formatted.md`
- Content Type: `text/markdown`
- Size: 1,133 bytes
- Attachment ID: `019c591d-d9b4-7591-8d2f-35cc10095cdb`
- **Extraction Strategy**: `text_native` ✅
- **Extracted**: Full markdown text (1,133 chars, 63 lines)

### Step 4: Upload JPEG File
- Filename: `jpeg-with-exif.jpg`
- Content Type: `image/jpeg`
- Size: 197,243 bytes
- Attachment ID: `019c591e-197e-76b1-abd9-6420cbea31d6`
- **Extraction Strategy**: `vision` ✅
- **Extracted**: EXIF metadata (GPS coordinates, camera info, datetime)

### Step 5: List Attachments
- **Total Count**: 3 ✅
- All attachments listed with status "completed"
- All extraction strategies verified via get_attachment

## Expected vs Actual Results

| Expected | Actual | Status |
|----------|--------|--------|
| 3 attachments | 3 attachments | ✅ PASS |
| Python: code_ast | code_ast | ✅ PASS |
| Markdown: text_native | text_native | ✅ PASS |
| JPEG: vision | vision | ✅ PASS |

## Observations

1. All three file types uploaded successfully to a single note
2. Each file type assigned appropriate extraction strategy automatically
3. Extraction completed for all files (status="completed")
4. Python file: AST analysis extracted 5 declarations
5. Markdown file: Full text extracted (1,133 chars)
6. JPEG file: EXIF metadata extracted (GPS, camera, datetime)
7. list_attachments returns all 3 files
8. get_attachment confirms extraction strategies for each

## Conclusion

**PASS** ✅ - All acceptance criteria met. Multiple file types successfully uploaded to one note with correct extraction strategies assigned and processing completed.

---
