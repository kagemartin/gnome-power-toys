/// Action produced by the activator in response to a keypress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivatorAction {
    /// Snap focused window to `zone_index`; `dismiss` tells the view to close.
    Snap { zone_index: u32, span: bool, dismiss: bool },
    /// Close the overlay without snapping.
    Dismiss,
    /// Ignore the key (no-op; overlay stays open).
    Ignore,
}

/// Compute the action for a given key press.
///
/// * `key_name` — GDK key name (e.g. "1", "KP_1", "Escape", "a").
/// * `shift` — true if Shift is held.
/// * `zone_count` — number of zones in the active layout (keys > zone_count are ignored).
/// * `paused` — if true, only Escape dismisses; digits are ignored.
pub fn handle_key(key_name: &str, shift: bool, zone_count: u32, paused: bool) -> ActivatorAction {
    if key_name == "Escape" {
        return ActivatorAction::Dismiss;
    }
    if paused {
        return ActivatorAction::Ignore;
    }
    let digit = parse_digit(key_name);
    if let Some(d) = digit {
        if d >= 1 && d <= zone_count {
            return ActivatorAction::Snap { zone_index: d, span: shift, dismiss: !shift };
        }
        return ActivatorAction::Ignore;
    }
    ActivatorAction::Dismiss
}

fn parse_digit(key_name: &str) -> Option<u32> {
    match key_name {
        "1" | "KP_1" => Some(1),
        "2" | "KP_2" => Some(2),
        "3" | "KP_3" => Some(3),
        "4" | "KP_4" => Some(4),
        "5" | "KP_5" => Some(5),
        "6" | "KP_6" => Some(6),
        "7" | "KP_7" => Some(7),
        "8" | "KP_8" => Some(8),
        "9" | "KP_9" => Some(9),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digit_snaps_and_dismisses() {
        assert_eq!(
            handle_key("2", false, 4, false),
            ActivatorAction::Snap { zone_index: 2, span: false, dismiss: true }
        );
    }

    #[test]
    fn shift_digit_snaps_and_stays_open() {
        assert_eq!(
            handle_key("3", true, 4, false),
            ActivatorAction::Snap { zone_index: 3, span: true, dismiss: false }
        );
    }

    #[test]
    fn keypad_digit_accepted() {
        assert_eq!(
            handle_key("KP_5", false, 9, false),
            ActivatorAction::Snap { zone_index: 5, span: false, dismiss: true }
        );
    }

    #[test]
    fn digit_above_zone_count_ignored() {
        assert_eq!(handle_key("5", false, 3, false), ActivatorAction::Ignore);
    }

    #[test]
    fn escape_dismisses() {
        assert_eq!(handle_key("Escape", false, 4, false), ActivatorAction::Dismiss);
    }

    #[test]
    fn other_key_dismisses() {
        assert_eq!(handle_key("a", false, 4, false), ActivatorAction::Dismiss);
    }

    #[test]
    fn paused_ignores_digits_but_escape_works() {
        assert_eq!(handle_key("2", false, 4, true), ActivatorAction::Ignore);
        assert_eq!(handle_key("Escape", false, 4, true), ActivatorAction::Dismiss);
    }
}
