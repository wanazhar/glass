//! Bounded PDF output for the Glass-owned native runtime.
//!
//! The native renderer already owns a deterministic semantic document and
//! software surface. This module emits a small, standards-valid text PDF from
//! that document without invoking a browser process or introducing a PDF
//! dependency into the workspace. It deliberately keeps the output bounded;
//! the native screenshot surface remains the authoritative visual capture.

use super::session::{BrowserResult, PdfOptions, SemanticObservation};
use base64::Engine as _;

const POINTS_PER_INCH: f64 = 72.0;
const DEFAULT_PAPER_WIDTH_INCHES: f64 = 8.5;
const DEFAULT_PAPER_HEIGHT_INCHES: f64 = 11.0;
const MAX_PAPER_DIMENSION_INCHES: f64 = 100.0;
const MAX_TEXT_BYTES: usize = 64 * 1024;
const MAX_PDF_BYTES: usize = crate::browser_backend::MAX_CAPTURE_BYTES;

/// Render one native semantic observation as base64-encoded PDF bytes.
pub(crate) fn render(
    observation: &SemanticObservation,
    options: &PdfOptions,
) -> BrowserResult<String> {
    let width = bounded_dimension(
        options.paper_width,
        DEFAULT_PAPER_WIDTH_INCHES,
        "paperWidth",
    )? * POINTS_PER_INCH;
    let height = bounded_dimension(
        options.paper_height,
        DEFAULT_PAPER_HEIGHT_INCHES,
        "paperHeight",
    )? * POINTS_PER_INCH;
    let scale = bounded_scale(options.scale)?;
    let margin_top = bounded_margin(options.margin_top, "marginTop")? * POINTS_PER_INCH;
    let margin_bottom = bounded_margin(options.margin_bottom, "marginBottom")? * POINTS_PER_INCH;
    let margin_left = bounded_margin(options.margin_left, "marginLeft")? * POINTS_PER_INCH;
    let margin_right = bounded_margin(options.margin_right, "marginRight")? * POINTS_PER_INCH;
    if margin_top + margin_bottom >= height || margin_left + margin_right >= width {
        return Err("native PDF margins leave no printable area".into());
    }

    let mut source = observation.text.as_deref().unwrap_or_default().to_owned();
    if source.len() > MAX_TEXT_BYTES {
        let mut end = MAX_TEXT_BYTES;
        while end > 0 && !source.is_char_boundary(end) {
            end -= 1;
        }
        source.truncate(end);
    }

    let font_size = 10.0 * scale;
    let line_height = 12.0 * scale;
    let printable_height = height - margin_top - margin_bottom;
    let max_lines = (printable_height / line_height).floor() as usize;
    let mut lines = Vec::with_capacity(max_lines.min(1024));
    if options.display_header_footer == Some(true) {
        lines.push(format_pdf_line(&observation.page.title));
    }
    for line in source.lines() {
        if lines.len() >= max_lines {
            break;
        }
        lines.push(format_pdf_line(line));
    }
    if options.display_header_footer == Some(true) && lines.len() < max_lines {
        lines.push(format_pdf_line(&observation.page.url));
    }

    let mut content = String::new();
    content.push_str("BT\n");
    content.push_str(&format!("/F1 {font_size:.2} Tf\n"));
    let mut y = height - margin_top - font_size;
    for line in lines {
        content.push_str(&format!("1 0 0 1 {margin_left:.2} {y:.2} Tm\n"));
        content.push('(');
        content.push_str(&line);
        content.push_str(") Tj\n");
        y -= line_height;
    }
    content.push_str("ET\n");

    let pdf = build_pdf(width, height, content.as_bytes())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(pdf))
}

fn bounded_dimension(value: Option<f64>, default: f64, field: &str) -> BrowserResult<f64> {
    let value = value.unwrap_or(default);
    if !value.is_finite() || value <= 0.0 || value > MAX_PAPER_DIMENSION_INCHES {
        return Err(format!(
            "native PDF {field} must be finite and between 0 and {MAX_PAPER_DIMENSION_INCHES} inches"
        )
        .into());
    }
    Ok(value)
}

fn bounded_scale(value: Option<f64>) -> BrowserResult<f64> {
    let value = value.unwrap_or(1.0);
    if !value.is_finite() || value <= 0.0 || value > 4.0 {
        return Err("native PDF scale must be finite and between 0 and 4".into());
    }
    Ok(value)
}

fn bounded_margin(value: Option<f64>, field: &str) -> BrowserResult<f64> {
    let value = value.unwrap_or(0.0);
    if !value.is_finite() || !(0.0..=MAX_PAPER_DIMENSION_INCHES).contains(&value) {
        return Err(format!(
            "native PDF {field} must be finite and between 0 and {MAX_PAPER_DIMENSION_INCHES} inches"
        )
        .into());
    }
    Ok(value)
}

fn format_pdf_line(value: &str) -> String {
    let mut line = String::with_capacity(value.len().min(1024));
    for character in value.chars() {
        match character {
            '\\' => line.push_str("\\\\"),
            '(' => line.push_str("\\("),
            ')' => line.push_str("\\)"),
            '\n' | '\r' => line.push(' '),
            character if character.is_ascii() && !character.is_ascii_control() => {
                line.push(character)
            }
            _ => line.push('?'),
        }
    }
    line
}

fn build_pdf(width: f64, height: f64, content: &[u8]) -> BrowserResult<Vec<u8>> {
    let mut pdf = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = vec![0usize];
    append_object(&mut pdf, &mut offsets, b"<< /Type /Catalog /Pages 2 0 R >>");
    append_object(
        &mut pdf,
        &mut offsets,
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    );
    append_object(
        &mut pdf,
        &mut offsets,
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width:.2} {height:.2}] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>"
        )
        .as_bytes(),
    );
    append_object(
        &mut pdf,
        &mut offsets,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    );
    let stream_header = format!("<< /Length {} >>\nstream\n", content.len());
    let mut stream = stream_header.into_bytes();
    stream.extend_from_slice(content);
    stream.extend_from_slice(b"endstream");
    append_object(&mut pdf, &mut offsets, &stream);

    let xref_offset = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n", offsets.len()).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
            offsets.len()
        )
        .as_bytes(),
    );
    if pdf.len() > MAX_PDF_BYTES {
        return Err(format!("native PDF exceeds the {}-byte output bound", MAX_PDF_BYTES).into());
    }
    Ok(pdf)
}

fn append_object(pdf: &mut Vec<u8>, offsets: &mut Vec<usize>, body: &[u8]) {
    offsets.push(pdf.len());
    let number = offsets.len() - 1;
    pdf.extend_from_slice(format!("{number} 0 obj\n").as_bytes());
    pdf.extend_from_slice(body);
    pdf.extend_from_slice(b"\nendobj\n");
}

#[cfg(test)]
mod tests {
    use super::format_pdf_line;

    #[test]
    fn escapes_pdf_delimiters_and_bounds_unicode_to_safe_text() {
        assert_eq!(format_pdf_line("a\\(b)\u{2603}"), "a\\\\\\(b\\)?");
    }
}
