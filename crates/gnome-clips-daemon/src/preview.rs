const MAX_PREVIEW_LEN: usize = 200;

pub fn generate_preview(content: &[u8], content_type: &str) -> String {
    match content_type {
        "text/plain" => truncate_utf8(content, MAX_PREVIEW_LEN),
        "text/html" => {
            let raw = String::from_utf8_lossy(content);
            let stripped = strip_html_tags(&raw);
            truncate_str(&stripped, MAX_PREVIEW_LEN)
        }
        "text/markdown" => {
            let raw = String::from_utf8_lossy(content);
            let stripped = strip_markdown(&raw);
            truncate_str(&stripped, MAX_PREVIEW_LEN)
        }
        t if t.starts_with("image/") => "[Image]".to_string(),
        "application/file" => {
            let path = String::from_utf8_lossy(content);
            let filename = std::path::Path::new(path.trim())
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string());
            format!("[File: {}]", filename)
        }
        _ => truncate_utf8(content, MAX_PREVIEW_LEN),
    }
}

fn truncate_utf8(bytes: &[u8], max: usize) -> String {
    let s = String::from_utf8_lossy(bytes);
    truncate_str(&s, max)
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

/// Minimal HTML tag stripper — removes anything inside < >.
fn strip_html_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Minimal Markdown syntax stripper — removes #, **, *, `, >.
fn strip_markdown(md: &str) -> String {
    md.lines()
        .map(|line| {
            let l = line.trim_start_matches('#').trim();
            l.replace("**", "").replace('*', "").replace('`', "").replace('>', "")
        })
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_preview_truncates_at_200_chars() {
        let long: String = "a".repeat(300);
        let preview = generate_preview(long.as_bytes(), "text/plain");
        assert_eq!(preview.len(), 200);
    }

    #[test]
    fn text_preview_returns_full_short_text() {
        let preview = generate_preview(b"hello world", "text/plain");
        assert_eq!(preview, "hello world");
    }

    #[test]
    fn html_preview_strips_tags() {
        let html = b"<h1>Hello</h1><p>World</p>";
        let preview = generate_preview(html, "text/html");
        assert!(preview.contains("Hello"));
        assert!(preview.contains("World"));
        assert!(!preview.contains('<'));
    }

    #[test]
    fn image_preview_shows_placeholder() {
        let preview = generate_preview(b"fake png data", "image/png");
        assert_eq!(preview, "[Image]");
    }

    #[test]
    fn file_preview_shows_placeholder() {
        let preview = generate_preview(b"/home/user/report.pdf", "application/file");
        assert_eq!(preview, "[File: report.pdf]");
    }

    #[test]
    fn markdown_preview_strips_syntax() {
        let md = b"# Title\n\n**bold** text";
        let preview = generate_preview(md, "text/markdown");
        assert!(preview.contains("Title"));
        assert!(preview.contains("bold"));
        assert!(!preview.contains('#'));
        assert!(!preview.contains("**"));
    }
}
