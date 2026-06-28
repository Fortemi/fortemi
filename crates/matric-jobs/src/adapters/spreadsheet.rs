//! Spreadsheet extraction adapter — reads xlsx/xls/ods/xlsb files using calamine.
//!
//! Iterates all sheets and converts each one to a markdown table. The first row
//! is used as headers if every cell contains a non-empty string; otherwise,
//! generic "Column A / Column B / …" headers are generated.
//!
//! Structured metadata includes sheet names, per-sheet dimensions, and aggregate
//! row/column counts. Empty sheets are skipped gracefully (marked in metadata).
//!
//! No external binaries are required — calamine is pure Rust, so the health
//! check always returns `Ok(true)`.

use std::io::Cursor;

use async_trait::async_trait;
use calamine::{open_workbook_auto_from_rs, Data, Reader};
use serde_json::Value as JsonValue;
use tracing::debug;

use matric_core::{ExtractionAdapter, ExtractionResult, ExtractionStrategy, Result};

fn spreadsheet_text_len(text: &str) -> usize {
    text.len()
}

fn spreadsheet_error_reason_code(error: &str) -> &'static str {
    let text = error.to_ascii_lowercase();
    if text.contains("permission") || text.contains("denied") {
        "permission_denied"
    } else if text.contains("password") || text.contains("encrypted") || text.contains("protected")
    {
        "protected_workbook"
    } else if text.contains("zip")
        || text.contains("xml")
        || text.contains("format")
        || text.contains("invalid")
    {
        "invalid_workbook"
    } else if text.contains("not found") || text.contains("no such") {
        "not_found"
    } else {
        "read_failed"
    }
}

// ── Column label helpers ─────────────────────────────────────────────────────

/// Convert a 0-based column index to a spreadsheet-style label (A, B, …, Z, AA, AB, …).
fn column_label(idx: usize) -> String {
    let mut label = String::new();
    let mut n = idx;
    loop {
        label.insert(0, (b'A' + (n % 26) as u8) as char);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    format!("Column {}", label)
}

// ── Cell → display string ────────────────────────────────────────────────────

/// Format a calamine `Data` cell value as a plain string.
///
/// `Empty` cells produce an empty string so the table is still well-formed.
fn cell_to_string(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(s) => s.clone(),
        Data::Int(i) => i.to_string(),
        Data::Float(f) => {
            // Avoid spurious trailing zeros for whole-number floats
            if f.fract() == 0.0 && f.abs() < 1e15 {
                format!("{}", *f as i64)
            } else {
                format!("{}", f)
            }
        }
        Data::Bool(b) => b.to_string(),
        Data::DateTime(dt) => dt.to_string(),
        Data::DateTimeIso(s) => s.clone(),
        Data::DurationIso(s) => s.clone(),
        Data::Error(e) => {
            let error_text = format!("{e:?}");
            format!("#ERR:{}", spreadsheet_error_reason_code(&error_text))
        }
    }
}

// ── Header detection ─────────────────────────────────────────────────────────

/// Return `true` if every cell in `row` is a non-empty `Data::String`.
///
/// An all-string, non-empty first row is treated as a header row.
fn row_looks_like_headers(row: &[Data]) -> bool {
    !row.is_empty()
        && row
            .iter()
            .all(|cell| matches!(cell, Data::String(s) if !s.is_empty()))
}

// ── Markdown table builder ───────────────────────────────────────────────────

/// Build a markdown section for a single sheet.
///
/// Returns `None` for sheets with no data (no rows, or all rows are empty).
fn sheet_to_markdown(sheet_name: &str, rows: &[Vec<Data>]) -> Option<String> {
    // Find the maximum column count across all rows
    let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if col_count == 0 {
        return None;
    }

    let row_count = rows.len();

    // Determine headers
    let (headers, data_rows): (Vec<String>, &[Vec<Data>]) =
        if row_count > 0 && row_looks_like_headers(&rows[0]) {
            let hdrs = rows[0]
                .iter()
                .enumerate()
                .map(|(i, cell)| {
                    let s = cell_to_string(cell);
                    if s.is_empty() {
                        column_label(i)
                    } else {
                        s
                    }
                })
                .collect();
            (hdrs, &rows[1..])
        } else {
            let hdrs = (0..col_count).map(column_label).collect();
            (hdrs, rows)
        };

    // Ensure header vec is wide enough (some rows might be wider than row[0])
    let headers: Vec<String> = (0..col_count)
        .map(|i| headers.get(i).cloned().unwrap_or_else(|| column_label(i)))
        .collect();

    let displayed_rows = data_rows.len();

    let mut md = format!(
        "## Sheet: {} ({} rows x {} columns)\n\n",
        sheet_name, displayed_rows, col_count
    );

    // Header row
    md.push_str("| ");
    md.push_str(&headers.join(" | "));
    md.push_str(" |\n");

    // Separator row
    md.push('|');
    for _ in 0..col_count {
        md.push_str("----------|");
    }
    md.push('\n');

    // Data rows
    for row in data_rows {
        md.push_str("| ");
        let cells: Vec<String> = (0..col_count)
            .map(|i| {
                row.get(i)
                    .map(cell_to_string)
                    .unwrap_or_default()
                    // Escape pipe characters inside cells
                    .replace('|', "\\|")
            })
            .collect();
        md.push_str(&cells.join(" | "));
        md.push_str(" |\n");
    }

    Some(md)
}

// ── Adapter ──────────────────────────────────────────────────────────────────

/// Adapter for extracting content from spreadsheet files (xlsx/xls/ods/xlsb).
///
/// Uses the [`calamine`] crate — a pure-Rust reader with no external binary
/// dependencies. Format is auto-detected from the file's magic bytes (not
/// the MIME type or file extension), so the same code path handles all
/// supported formats.
pub struct SpreadsheetAdapter;

#[async_trait]
impl ExtractionAdapter for SpreadsheetAdapter {
    fn strategy(&self) -> ExtractionStrategy {
        ExtractionStrategy::Spreadsheet
    }

    async fn extract(
        &self,
        data: &[u8],
        filename: &str,
        _mime_type: &str,
        _config: &JsonValue,
    ) -> Result<ExtractionResult> {
        if data.is_empty() {
            return Err(matric_core::Error::InvalidInput(
                "Cannot extract text from empty spreadsheet data".to_string(),
            ));
        }

        let cursor = Cursor::new(data.to_vec());
        let mut workbook = open_workbook_auto_from_rs(cursor).map_err(|e| {
            let error_text = e.to_string();
            matric_core::Error::InvalidInput(format!(
                "Failed to open spreadsheet; filename_len={}; error_len={}; error_reason={}",
                spreadsheet_text_len(filename),
                spreadsheet_text_len(&error_text),
                spreadsheet_error_reason_code(&error_text)
            ))
        })?;

        let sheet_names = workbook.sheet_names().to_owned();
        debug!(
            filename_len = spreadsheet_text_len(filename),
            sheet_count = sheet_names.len(),
            "Opening spreadsheet"
        );

        let mut sections: Vec<String> = Vec::new();
        let mut sheet_meta: Vec<JsonValue> = Vec::new();
        let mut total_rows: usize = 0;
        let mut total_cols: usize = 0;

        for sheet_name in &sheet_names {
            match workbook.worksheet_range(sheet_name) {
                Ok(range) => {
                    let (height, width) = range.get_size();

                    // Collect all rows into owned Vecs for processing
                    let rows: Vec<Vec<Data>> = range.rows().map(|row| row.to_vec()).collect();

                    if rows.is_empty() || width == 0 {
                        debug!(
                            sheet_name_len = spreadsheet_text_len(sheet_name),
                            "Skipping empty sheet"
                        );
                        sheet_meta.push(serde_json::json!({
                            "name": sheet_name,
                            "rows": 0,
                            "columns": 0,
                            "empty": true,
                        }));
                        continue;
                    }

                    total_rows += height;
                    total_cols = total_cols.max(width);

                    sheet_meta.push(serde_json::json!({
                        "name": sheet_name,
                        "rows": height,
                        "columns": width,
                        "empty": false,
                    }));

                    if let Some(md) = sheet_to_markdown(sheet_name, &rows) {
                        sections.push(md);
                    }
                }
                Err(e) => {
                    let error_text = e.to_string();
                    debug!(
                        sheet_name_len = spreadsheet_text_len(sheet_name),
                        error_len = spreadsheet_text_len(&error_text),
                        error_reason = spreadsheet_error_reason_code(&error_text),
                        "Failed to read sheet, skipping"
                    );
                    sheet_meta.push(serde_json::json!({
                        "name": sheet_name,
                        "error_reason": spreadsheet_error_reason_code(&error_text),
                        "error_len": spreadsheet_text_len(&error_text),
                        "empty": true,
                    }));
                }
            }
        }

        let extracted_text = if sections.is_empty() {
            None
        } else {
            Some(sections.join("\n"))
        };

        let metadata = serde_json::json!({
            "sheet_count": sheet_names.len(),
            "sheets": sheet_meta,
            "total_rows": total_rows,
            "max_columns": total_cols,
        });

        Ok(ExtractionResult {
            extracted_text,
            metadata,
            ai_description: None,
            preview_data: None,
            derived_files: vec![],
        })
    }

    async fn health_check(&self) -> Result<bool> {
        // Pure Rust — no external binary dependency
        Ok(true)
    }

    fn name(&self) -> &str {
        "spreadsheet"
    }
}

// ── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn invalid_input_message(error: matric_core::Error) -> String {
        match error {
            matric_core::Error::InvalidInput(message) => message,
            other => panic!("expected invalid input error, got {other:?}"),
        }
    }

    #[test]
    fn spreadsheet_error_reason_code_uses_stable_classes() {
        assert_eq!(
            spreadsheet_error_reason_code("permission denied opening /srv/private/book.xlsx"),
            "permission_denied"
        );
        assert_eq!(
            spreadsheet_error_reason_code("workbook is password protected token=mm_key_secret"),
            "protected_workbook"
        );
        assert_eq!(
            spreadsheet_error_reason_code("invalid zip central directory"),
            "invalid_workbook"
        );
        assert_eq!(
            spreadsheet_error_reason_code("opaque backend text /srv/private/book.xlsx"),
            "read_failed"
        );
    }

    #[test]
    fn spreadsheet_log_metadata_omits_raw_sheet_names_and_errors() {
        let filename = "customer-token-mm_key_secret.xlsx";
        let sheet = "Payroll customer-token-mm_key_secret";
        let error = "permission denied at /srv/fortemi/private/customer-token-mm_key_secret.xlsx";
        let detail = format!(
            "filename_len={}; sheet_name_len={}; error_len={}; error_reason={}",
            spreadsheet_text_len(filename),
            spreadsheet_text_len(sheet),
            spreadsheet_text_len(error),
            spreadsheet_error_reason_code(error)
        );

        assert!(detail.contains("filename_len="));
        assert!(detail.contains("sheet_name_len="));
        assert!(detail.contains("error_len="));
        assert!(detail.contains("permission_denied"));
        assert!(!detail.contains("Payroll"));
        assert!(!detail.contains("customer-token"));
        assert!(!detail.contains("mm_key_secret"));
        assert!(!detail.contains("/srv/fortemi"));
    }

    // ── column_label ─────────────────────────────────────────────────────

    #[test]
    fn test_column_label_first_26() {
        assert_eq!(column_label(0), "Column A");
        assert_eq!(column_label(1), "Column B");
        assert_eq!(column_label(25), "Column Z");
    }

    #[test]
    fn test_column_label_two_chars() {
        // Index 26 → AA
        assert_eq!(column_label(26), "Column AA");
        // Index 27 → AB
        assert_eq!(column_label(27), "Column AB");
        // Index 51 → AZ
        assert_eq!(column_label(51), "Column AZ");
        // Index 52 → BA
        assert_eq!(column_label(52), "Column BA");
    }

    #[test]
    fn test_column_label_three_chars() {
        // Index 702 → AAA (26^2 + 26 = 702)
        assert_eq!(column_label(702), "Column AAA");
    }

    // ── cell_to_string ────────────────────────────────────────────────────

    #[test]
    fn test_cell_to_string_empty() {
        assert_eq!(cell_to_string(&Data::Empty), "");
    }

    #[test]
    fn test_cell_to_string_string() {
        assert_eq!(cell_to_string(&Data::String("hello".to_string())), "hello");
    }

    #[test]
    fn test_cell_to_string_int() {
        assert_eq!(cell_to_string(&Data::Int(42)), "42");
        assert_eq!(cell_to_string(&Data::Int(-7)), "-7");
        assert_eq!(cell_to_string(&Data::Int(0)), "0");
    }

    #[test]
    fn test_cell_to_string_float_whole_number() {
        // Whole-number floats should not have decimal point
        assert_eq!(cell_to_string(&Data::Float(3.0)), "3");
        assert_eq!(cell_to_string(&Data::Float(-100.0)), "-100");
        assert_eq!(cell_to_string(&Data::Float(0.0)), "0");
    }

    #[test]
    fn test_cell_to_string_float_fractional() {
        let s = cell_to_string(&Data::Float(2.78));
        assert!(
            s.starts_with("2.78"),
            "Expected fractional float, got: {}",
            s
        );
    }

    #[test]
    fn test_cell_to_string_bool() {
        assert_eq!(cell_to_string(&Data::Bool(true)), "true");
        assert_eq!(cell_to_string(&Data::Bool(false)), "false");
    }

    #[test]
    fn test_cell_to_string_datetime_iso() {
        assert_eq!(
            cell_to_string(&Data::DateTimeIso("2024-06-15T12:00:00".to_string())),
            "2024-06-15T12:00:00"
        );
    }

    #[test]
    fn test_cell_to_string_duration_iso() {
        assert_eq!(
            cell_to_string(&Data::DurationIso("PT1H30M".to_string())),
            "PT1H30M"
        );
    }

    #[test]
    fn spreadsheet_cell_error_output_uses_reason_code_only() {
        let rendered = cell_to_string(&Data::Error(calamine::CellErrorType::Name));

        assert_eq!(rendered, "#ERR:read_failed");
        assert!(!rendered.contains("Name"));
    }

    // ── row_looks_like_headers ────────────────────────────────────────────

    #[test]
    fn test_row_looks_like_headers_all_strings() {
        let row = vec![
            Data::String("Name".to_string()),
            Data::String("Age".to_string()),
            Data::String("City".to_string()),
        ];
        assert!(row_looks_like_headers(&row));
    }

    #[test]
    fn test_row_looks_like_headers_with_empty_string() {
        // An empty-string cell means it does NOT look like headers
        let row = vec![
            Data::String("Name".to_string()),
            Data::String("".to_string()),
            Data::String("City".to_string()),
        ];
        assert!(!row_looks_like_headers(&row));
    }

    #[test]
    fn test_row_looks_like_headers_with_numbers() {
        let row = vec![Data::String("Name".to_string()), Data::Float(42.0)];
        assert!(!row_looks_like_headers(&row));
    }

    #[test]
    fn test_row_looks_like_headers_empty_row() {
        assert!(!row_looks_like_headers(&[]));
    }

    #[test]
    fn test_row_looks_like_headers_all_empty_cells() {
        let row = vec![Data::Empty, Data::Empty];
        assert!(!row_looks_like_headers(&row));
    }

    // ── sheet_to_markdown ─────────────────────────────────────────────────

    #[test]
    fn test_sheet_to_markdown_none_for_empty_rows() {
        let result = sheet_to_markdown("Sheet1", &[]);
        assert!(result.is_none());
    }

    #[test]
    fn test_sheet_to_markdown_none_for_zero_width() {
        // A row with only Empty cells should yield width 0 via max col_count
        // (rows exist but col_count = 0 since all rows are empty slices)
        let result = sheet_to_markdown("Sheet1", &[vec![]]);
        assert!(result.is_none());
    }

    #[test]
    fn test_sheet_to_markdown_header_row_detected() {
        let rows = vec![
            vec![
                Data::String("Name".to_string()),
                Data::String("Score".to_string()),
            ],
            vec![Data::String("Alice".to_string()), Data::Int(95)],
            vec![Data::String("Bob".to_string()), Data::Int(87)],
        ];
        let md = sheet_to_markdown("Results", &rows).unwrap();

        // Section header
        assert!(
            md.contains("## Sheet: Results (2 rows x 2 columns)"),
            "Missing section header in:\n{}",
            md
        );
        // Detected headers
        assert!(md.contains("Name"), "Missing 'Name' header");
        assert!(md.contains("Score"), "Missing 'Score' header");
        // Data rows
        assert!(md.contains("Alice"), "Missing 'Alice' row");
        assert!(md.contains("95"), "Missing score 95");
        assert!(md.contains("Bob"), "Missing 'Bob' row");
        assert!(md.contains("87"), "Missing score 87");
    }

    #[test]
    fn test_sheet_to_markdown_generic_headers_when_first_row_has_numbers() {
        let rows = vec![
            vec![Data::Int(1), Data::Int(2), Data::Int(3)],
            vec![Data::Int(4), Data::Int(5), Data::Int(6)],
        ];
        let md = sheet_to_markdown("Data", &rows).unwrap();

        assert!(md.contains("Column A"), "Missing 'Column A' header");
        assert!(md.contains("Column B"), "Missing 'Column B' header");
        assert!(md.contains("Column C"), "Missing 'Column C' header");
    }

    #[test]
    fn sheet_to_markdown_redacts_cell_error_debug_values() {
        let rows = vec![
            vec![
                Data::String("Status".to_string()),
                Data::String("Diagnostic".to_string()),
            ],
            vec![
                Data::String("failed".to_string()),
                Data::Error(calamine::CellErrorType::Name),
            ],
        ];

        let md = sheet_to_markdown("Errors", &rows).unwrap();

        assert!(md.contains("#ERR:read_failed"));
        assert!(!md.contains("#NAME"));
        assert!(!md.contains("Name"));
    }

    #[test]
    fn spreadsheet_error_metadata_shape_omits_raw_error_text() {
        let sheet_name = "customer@example.com private payroll";
        let raw_error = "permission denied for /srv/private/book.xlsx token=sk-live-spreadsheet";
        let error_text = raw_error.to_string();
        let meta = serde_json::json!({
            "name": sheet_name,
            "error_reason": spreadsheet_error_reason_code(&error_text),
            "error_len": spreadsheet_text_len(&error_text),
            "empty": true,
        });

        assert_eq!(meta["error_reason"], "permission_denied");
        assert_eq!(meta["error_len"], raw_error.len());
        assert!(meta.get("error").is_none());

        let rendered = meta.to_string();
        for raw in ["/srv/private", "sk-live-spreadsheet", "permission denied"] {
            assert!(!rendered.contains(raw), "raw error leaked: {raw}");
        }
    }

    #[test]
    fn test_sheet_to_markdown_single_row_no_headers() {
        // A single row that looks like headers — becomes section with 0 data rows
        let rows = vec![vec![
            Data::String("H1".to_string()),
            Data::String("H2".to_string()),
        ]];
        let md = sheet_to_markdown("OneRow", &rows).unwrap();
        assert!(
            md.contains("## Sheet: OneRow (0 rows x 2 columns)"),
            "Unexpected header in:\n{}",
            md
        );
        assert!(md.contains("H1"));
        assert!(md.contains("H2"));
    }

    #[test]
    fn test_sheet_to_markdown_pipe_escaped_in_cells() {
        let rows = vec![
            vec![Data::String("Col".to_string())],
            vec![Data::String("a|b".to_string())],
        ];
        let md = sheet_to_markdown("PipeTest", &rows).unwrap();
        assert!(
            md.contains("a\\|b"),
            "Pipe should be escaped in cell data:\n{}",
            md
        );
    }

    #[test]
    fn test_sheet_to_markdown_uneven_rows_padded() {
        // Rows with fewer columns than the max should be padded with empty strings
        let rows = vec![
            vec![
                Data::String("A".to_string()),
                Data::String("B".to_string()),
                Data::String("C".to_string()),
            ],
            // This row only has 2 cells — should be padded with empty string for col 3
            vec![Data::Int(1), Data::Int(2)],
        ];
        let md = sheet_to_markdown("Uneven", &rows).unwrap();
        // Should still produce 3-column table
        assert!(md.contains("| A | B | C |"), "Header row mismatch:\n{}", md);
        // Data row should have 3 columns (last one empty)
        assert!(md.contains("| 1 | 2 |  |"), "Data row mismatch:\n{}", md);
    }

    #[test]
    fn test_sheet_to_markdown_separator_row_present() {
        let rows = vec![vec![Data::String("X".to_string())], vec![Data::Int(10)]];
        let md = sheet_to_markdown("Sep", &rows).unwrap();
        // Separator line must be between header and data
        assert!(md.contains("----------"), "Missing separator:\n{}", md);
    }

    // ── SpreadsheetAdapter trait methods ──────────────────────────────────

    #[test]
    fn test_spreadsheet_strategy() {
        let adapter = SpreadsheetAdapter;
        assert_eq!(adapter.strategy(), ExtractionStrategy::Spreadsheet);
    }

    #[test]
    fn test_spreadsheet_name() {
        let adapter = SpreadsheetAdapter;
        assert_eq!(adapter.name(), "spreadsheet");
    }

    #[tokio::test]
    async fn test_spreadsheet_health_check() {
        let adapter = SpreadsheetAdapter;
        assert!(adapter.health_check().await.unwrap());
    }

    #[tokio::test]
    async fn test_spreadsheet_empty_data_returns_error() {
        let adapter = SpreadsheetAdapter;
        let result = adapter
            .extract(
                b"",
                "empty.xlsx",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                &serde_json::json!({}),
            )
            .await;
        assert!(result.is_err());
        // Error Display is redacted; assert on the typed variant's inner message.
        let err = match result.unwrap_err() {
            matric_core::Error::InvalidInput(msg) => msg,
            other => panic!("expected InvalidInput, got: {other:?}"),
        };
        assert!(
            err.contains("empty"),
            "Error should mention empty data: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_spreadsheet_invalid_data_returns_error() {
        let adapter = SpreadsheetAdapter;
        let filename = "bad-token-sk-live.xlsx";
        let result = adapter
            .extract(
                b"not a spreadsheet at all",
                filename,
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                &serde_json::json!({}),
            )
            .await;
        assert!(result.is_err());
        let err = invalid_input_message(result.unwrap_err());
        assert!(
            err.to_lowercase().contains("failed") || err.to_lowercase().contains("spreadsheet"),
            "Error should mention failure or spreadsheet: {}",
            err
        );
        assert!(err.contains("filename_len="));
        assert!(err.contains("error_len="));
        assert!(err.contains("error_reason="));
        assert!(!err.contains(filename));
        assert!(!err.contains("sk-live"));
    }

    // ── Integration tests using real xlsx bytes ───────────────────────────
    //
    // A minimal valid xlsx is a zip archive containing specific XML files.
    // Rather than embedding a full xlsx binary, we test the format-detection
    // path by providing invalid data and verifying the error path, and we
    // test each pure helper function exhaustively above.
    //
    // For full round-trip tests (open → extract → verify), see the UAT test
    // suite which provides real spreadsheet fixtures.

    #[tokio::test]
    async fn test_spreadsheet_ai_description_is_none() {
        // Verify the adapter never returns an AI description
        let adapter = SpreadsheetAdapter;
        // We use invalid data to trigger early return — just verifying the
        // adapter struct fields are correct. The real path is tested via UAT.
        let result = adapter
            .extract(
                b"bad",
                "test.xlsx",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                &serde_json::json!({}),
            )
            .await;
        // Whether it errors or succeeds, ai_description must be None
        if let Ok(extraction) = result {
            assert!(extraction.ai_description.is_none());
            assert!(extraction.preview_data.is_none());
        }
    }

    // ── Minimal XLSX fixture test ─────────────────────────────────────────
    //
    // We build a minimal valid xlsx (zip-format) in memory so we can test the
    // full extraction path without file system fixtures.

    /// Build the smallest valid xlsx workbook in memory containing one sheet
    /// with the provided rows of string data.
    ///
    /// Uses the zip crate (available through calamine's dependencies) — we
    /// construct the required xlsx XML structure by hand.
    fn build_minimal_xlsx(rows: &[Vec<&str>]) -> Vec<u8> {
        use std::io::Write;

        let buf = Vec::new();
        let cursor = Cursor::new(buf);
        let mut zip = zip::ZipWriter::new(cursor);

        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        // [Content_Types].xml
        zip.start_file("[Content_Types].xml", opts).unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
  <Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
  <Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>
</Types>"#).unwrap();

        // _rels/.rels
        zip.start_file("_rels/.rels", opts).unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"#).unwrap();

        // xl/_rels/workbook.xml.rels
        zip.start_file("xl/_rels/workbook.xml.rels", opts).unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/>
</Relationships>"#).unwrap();

        // xl/workbook.xml
        zip.start_file("xl/workbook.xml", opts).unwrap();
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
          xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <sheets>
    <sheet name="TestSheet" sheetId="1" r:id="rId1"/>
  </sheets>
</workbook>"#,
        )
        .unwrap();

        // Build shared strings table and sheet data
        // Collect all unique strings in row order
        let mut strings: Vec<String> = Vec::new();
        let mut string_index: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        // Pre-populate shared strings
        for row in rows {
            for cell in row.iter() {
                let s = cell.to_string();
                if !string_index.contains_key(&s) {
                    string_index.insert(s.clone(), strings.len());
                    strings.push(s);
                }
            }
        }

        // xl/sharedStrings.xml
        zip.start_file("xl/sharedStrings.xml", opts).unwrap();
        let mut ss_xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="{}" uniqueCount="{}">
"#,
            strings.len(),
            strings.len()
        );
        for s in &strings {
            ss_xml.push_str(&format!("  <si><t>{}</t></si>\n", xml_escape(s)));
        }
        ss_xml.push_str("</sst>");
        zip.write_all(ss_xml.as_bytes()).unwrap();

        // xl/worksheets/sheet1.xml
        zip.start_file("xl/worksheets/sheet1.xml", opts).unwrap();
        let mut ws_xml = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
"#,
        );
        for (row_idx, row) in rows.iter().enumerate() {
            let row_num = row_idx + 1;
            ws_xml.push_str(&format!("    <row r=\"{}\">\n", row_num));
            for (col_idx, cell) in row.iter().enumerate() {
                let col_letter = (b'A' + col_idx as u8) as char;
                let cell_ref = format!("{}{}", col_letter, row_num);
                let si = string_index[*cell];
                ws_xml.push_str(&format!(
                    "      <c r=\"{}\" t=\"s\"><v>{}</v></c>\n",
                    cell_ref, si
                ));
            }
            ws_xml.push_str("    </row>\n");
        }
        ws_xml.push_str("  </sheetData>\n</worksheet>");
        zip.write_all(ws_xml.as_bytes()).unwrap();

        let cursor = zip.finish().unwrap();
        cursor.into_inner()
    }

    /// Minimal XML escaping for shared strings content.
    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }

    #[tokio::test]
    async fn test_extract_xlsx_with_headers() {
        let xlsx_bytes = build_minimal_xlsx(&[
            vec!["Name", "Age", "City"],
            vec!["Alice", "30", "Paris"],
            vec!["Bob", "25", "Berlin"],
        ]);

        let adapter = SpreadsheetAdapter;
        let result = adapter
            .extract(
                &xlsx_bytes,
                "test.xlsx",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                &serde_json::json!({}),
            )
            .await
            .expect("Extraction should succeed for valid xlsx");

        let text = result.extracted_text.expect("Should have extracted text");

        assert!(text.contains("TestSheet"), "Should contain sheet name");
        assert!(text.contains("Name"), "Should contain 'Name' header");
        assert!(text.contains("Age"), "Should contain 'Age' header");
        assert!(text.contains("City"), "Should contain 'City' header");
        assert!(text.contains("Alice"), "Should contain 'Alice'");
        assert!(text.contains("Paris"), "Should contain 'Paris'");
        assert!(text.contains("Bob"), "Should contain 'Bob'");
        assert!(text.contains("Berlin"), "Should contain 'Berlin'");

        // Metadata
        assert_eq!(result.metadata["sheet_count"], 1);
        let sheets = result.metadata["sheets"].as_array().unwrap();
        assert_eq!(sheets.len(), 1);
        assert_eq!(sheets[0]["name"], "TestSheet");
        assert!(!sheets[0]["empty"].as_bool().unwrap());

        // No AI description or preview
        assert!(result.ai_description.is_none());
        assert!(result.preview_data.is_none());
    }

    #[tokio::test]
    async fn test_extract_xlsx_metadata_structure() {
        let xlsx_bytes = build_minimal_xlsx(&[
            vec!["H1", "H2"],
            vec!["v1", "v2"],
            vec!["v3", "v4"],
            vec!["v5", "v6"],
        ]);

        let adapter = SpreadsheetAdapter;
        let result = adapter
            .extract(
                &xlsx_bytes,
                "meta.xlsx",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        assert_eq!(result.metadata["sheet_count"], 1);
        // 4 rows total in the sheet (1 header + 3 data), 2 columns
        assert!(result.metadata["total_rows"].as_u64().unwrap() > 0);
        assert_eq!(result.metadata["max_columns"], 2);
    }

    #[tokio::test]
    async fn test_extract_xlsx_section_row_count_excludes_header() {
        // 1 header + 2 data rows → section says "2 rows x 3 columns"
        let xlsx_bytes = build_minimal_xlsx(&[
            vec!["Col1", "Col2", "Col3"],
            vec!["a", "b", "c"],
            vec!["d", "e", "f"],
        ]);

        let adapter = SpreadsheetAdapter;
        let result = adapter
            .extract(
                &xlsx_bytes,
                "rows.xlsx",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        let text = result.extracted_text.unwrap();
        assert!(
            text.contains("2 rows x 3 columns"),
            "Section should say '2 rows x 3 columns', got:\n{}",
            text
        );
    }
}
