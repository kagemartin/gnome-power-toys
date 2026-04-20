//! Pure helpers used by widgets. Kept free of GTK types so they can be
//! exercised by unit tests without initializing a display.

use crate::dbus::ClipSummary;

pub fn type_icon(mime: &str) -> &'static str {
    match mime {
        "text/plain" => "🔤",
        "text/html" => "📋",
        "text/markdown" => "📝",
        "application/file" => "📄",
        m if m.starts_with("image/") => "🖼️",
        _ => "📋",
    }
}

pub fn friendly_type(mime: &str) -> &'static str {
    match mime {
        "text/plain" => "Text",
        "text/html" => "HTML",
        "text/markdown" => "Markdown",
        "application/file" => "File",
        m if m.starts_with("image/") => "Image",
        _ => "Clip",
    }
}

/// Format `created_at` (unix seconds) relative to `now_secs`.
pub fn friendly_age_at(now_secs: i64, created_at: i64) -> String {
    let secs = now_secs.saturating_sub(created_at);
    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{} min ago", secs / 60)
    } else if secs < 86400 {
        format!("{} hr ago", secs / 3600)
    } else {
        format!("{} days ago", secs / 86400)
    }
}

/// Convenience using the current system time.
pub fn friendly_age(created_at: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    friendly_age_at(now, created_at)
}

/// Map the filter-bar toggle state to the daemon's filter string.
/// Priority matches spec: pinned > text > image > file > html > markdown > all.
pub fn filter_for_state(
    pinned: bool,
    text: bool,
    image: bool,
    file: bool,
    html: bool,
    markdown: bool,
) -> &'static str {
    if pinned {
        "pinned"
    } else if text {
        "text/plain"
    } else if image {
        "image/*"
    } else if file {
        "application/file"
    } else if html {
        "text/html"
    } else if markdown {
        "text/markdown"
    } else {
        ""
    }
}

/// Sort pinned clips first (newest first within each group).
pub fn sort_clips<'a>(clips: &'a [ClipSummary]) -> Vec<&'a ClipSummary> {
    let mut out: Vec<&ClipSummary> = clips.iter().collect();
    out.sort_by_key(|c| (!c.pinned, -c.created_at));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(id: i64, pinned: bool, created_at: i64) -> ClipSummary {
        ClipSummary {
            id,
            content_type: "text/plain".into(),
            preview: String::new(),
            source_app: String::new(),
            created_at,
            pinned,
            tags: vec![],
        }
    }

    #[test]
    fn type_icon_maps_known_types() {
        assert_eq!(type_icon("text/plain"), "🔤");
        assert_eq!(type_icon("text/html"), "📋");
        assert_eq!(type_icon("text/markdown"), "📝");
        assert_eq!(type_icon("application/file"), "📄");
        assert_eq!(type_icon("image/png"), "🖼️");
        assert_eq!(type_icon("image/jpeg"), "🖼️");
        assert_eq!(type_icon("application/unknown"), "📋");
    }

    #[test]
    fn friendly_type_maps_known_types() {
        assert_eq!(friendly_type("text/plain"), "Text");
        assert_eq!(friendly_type("text/html"), "HTML");
        assert_eq!(friendly_type("text/markdown"), "Markdown");
        assert_eq!(friendly_type("application/file"), "File");
        assert_eq!(friendly_type("image/png"), "Image");
        assert_eq!(friendly_type("weird/thing"), "Clip");
    }

    #[test]
    fn friendly_age_just_now_under_60s() {
        assert_eq!(friendly_age_at(1000, 1000), "just now");
        assert_eq!(friendly_age_at(1059, 1000), "just now");
    }

    #[test]
    fn friendly_age_minutes() {
        assert_eq!(friendly_age_at(1060, 1000), "1 min ago");
        assert_eq!(friendly_age_at(3599, 1000), "43 min ago");
    }

    #[test]
    fn friendly_age_hours() {
        assert_eq!(friendly_age_at(3600, 0), "1 hr ago");
        assert_eq!(friendly_age_at(86_399, 0), "23 hr ago");
    }

    #[test]
    fn friendly_age_days() {
        assert_eq!(friendly_age_at(86_400, 0), "1 days ago");
        assert_eq!(friendly_age_at(3 * 86_400, 0), "3 days ago");
    }

    #[test]
    fn friendly_age_never_panics_on_future_timestamp() {
        // saturating_sub keeps us out of underflow territory.
        assert_eq!(friendly_age_at(100, 200), "just now");
    }

    #[test]
    fn filter_priority_pinned_wins() {
        assert_eq!(
            filter_for_state(true, true, true, true, true, true),
            "pinned"
        );
    }

    #[test]
    fn filter_priority_text_beats_image() {
        assert_eq!(
            filter_for_state(false, true, true, false, false, false),
            "text/plain"
        );
    }

    #[test]
    fn filter_all_when_no_toggles() {
        assert_eq!(
            filter_for_state(false, false, false, false, false, false),
            ""
        );
    }

    #[test]
    fn filter_each_type_maps_correctly() {
        assert_eq!(
            filter_for_state(false, false, true, false, false, false),
            "image/*"
        );
        assert_eq!(
            filter_for_state(false, false, false, true, false, false),
            "application/file"
        );
        assert_eq!(
            filter_for_state(false, false, false, false, true, false),
            "text/html"
        );
        assert_eq!(
            filter_for_state(false, false, false, false, false, true),
            "text/markdown"
        );
    }

    #[test]
    fn sort_clips_pins_first() {
        let clips = vec![
            clip(1, false, 100),
            clip(2, true, 50),
            clip(3, false, 200),
            clip(4, true, 150),
        ];
        let sorted = sort_clips(&clips);
        let ids: Vec<i64> = sorted.iter().map(|c| c.id).collect();
        // Pinned first (newest pinned = id 4, then id 2), then unpinned newest = id 3 then id 1.
        assert_eq!(ids, vec![4, 2, 3, 1]);
    }

    #[test]
    fn sort_clips_stable_ordering_within_group() {
        let clips = vec![clip(1, false, 10), clip(2, false, 20), clip(3, false, 15)];
        let sorted = sort_clips(&clips);
        let ids: Vec<i64> = sorted.iter().map(|c| c.id).collect();
        assert_eq!(ids, vec![2, 3, 1]);
    }

    #[test]
    fn sort_clips_empty_returns_empty() {
        let sorted = sort_clips(&[]);
        assert!(sorted.is_empty());
    }
}
