//! Terminal cell widths; never split a UTF-8 character or emit control codes.
use unicode_width::UnicodeWidthChar;
pub fn char_width(c: char) -> usize {
    c.width().unwrap_or(0)
}
pub fn width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}
pub fn clip(s: &str, limit: usize) -> String {
    let mut used = 0;
    s.chars()
        .filter(|c| !c.is_control())
        .take_while(|c| {
            used += c.width().unwrap_or(0);
            used <= limit
        })
        .collect()
}
pub fn wrap(s: &str, limit: usize) -> Vec<String> {
    let mut lines = vec![String::new()];
    let mut used = 0;
    let limit = limit.max(1);
    for c in s.chars() {
        if c == '\n' {
            lines.push(String::new());
            used = 0;
            continue;
        }
        if c.is_control() {
            continue;
        }
        let n = c.width().unwrap_or(0);
        if used + n > limit && used > 0 {
            lines.push(String::new());
            used = 0;
        }
        if n <= limit {
            lines.last_mut().unwrap().push(c);
            used += n;
        }
    }
    lines
}

/// Wrap plain prose at word boundaries, splitting oversized words by cell width.
pub fn wrap_words(s: &str, limit: usize) -> Vec<String> {
    let limit = limit.max(1);
    let mut rows = Vec::new();
    for paragraph in s.split('\n') {
        let mut row = String::new();
        for word in paragraph.split_whitespace() {
            if !row.is_empty() && width(&row) + 1 + width(word) > limit {
                rows.push(std::mem::take(&mut row));
            }
            if !row.is_empty() {
                row.push(' ');
            }
            let chunks = wrap(word, limit);
            for (index, chunk) in chunks.iter().enumerate() {
                if index > 0 {
                    rows.push(std::mem::take(&mut row));
                }
                row.push_str(chunk);
            }
        }
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prose_wraps_at_words_and_bounds_long_tokens() {
        assert_eq!(wrap_words("773 in · 151 out", 19), ["773 in · 151 out"]);
        assert_eq!(
            wrap_words("Reading and editing files", 19),
            ["Reading and editing", "files"]
        );
        for line in wrap_words("界界界界 abcdefghijklmnopqrstuvwxyz", 5) {
            assert!(width(&line) <= 5);
        }
    }
    #[test]
    fn wide_and_combining_text_obeys_cell_limits() {
        assert_eq!(width("界e\u{301}"), 3);
        assert_eq!(clip("界ab", 3), "界a");
        for line in wrap("界界abc\ne\u{301}", 3) {
            assert!(width(&line) <= 3);
        }
        assert_eq!(clip("\x1b\nhello", 3), "hel");
    }
}
