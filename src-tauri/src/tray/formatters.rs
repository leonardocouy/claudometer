use crate::provider_view::{view_claude, view_codex};
use crate::types::UsageSnapshotBundle;
use chrono::format::Locale;
use chrono::{DateTime, FixedOffset, Local};

pub(crate) fn format_percent(value: Option<f64>) -> String {
    value
        .map(|v| format!("{}%", v.round() as i64))
        .unwrap_or_else(|| "--%".to_string())
}

/// Generate the tray title text based on usage snapshot.
/// Returns percentage for Ok state, "--%" for error states.
pub(crate) fn format_tray_title(
    track_claude: bool,
    track_codex: bool,
    snapshot: Option<&UsageSnapshotBundle>,
) -> String {
    if track_claude && track_codex {
        let claude = snapshot
            .and_then(|s| s.claude.as_ref())
            .and_then(view_claude)
            .map(|v| v.session_percent)
            .map(|v| format!("{}%", v.round() as i64))
            .unwrap_or_else(|| "--%".to_string());
        let codex = snapshot
            .and_then(|s| s.codex.as_ref())
            .and_then(view_codex)
            .map(|v| v.session_percent)
            .map(|v| format!("{}%", v.round() as i64))
            .unwrap_or_else(|| "--%".to_string());
        return format!("CL {claude} · CX {codex}");
    }

    if track_claude {
        let percent = snapshot
            .and_then(|s| s.claude.as_ref())
            .and_then(view_claude)
            .map(|v| v.session_percent);
        let value = percent
            .map(|v| format!("{}%", v.round() as i64))
            .unwrap_or_else(|| "--%".to_string());
        return format!("CL {value}");
    }
    if track_codex {
        let percent = snapshot
            .and_then(|s| s.codex.as_ref())
            .and_then(view_codex)
            .map(|v| v.session_percent);
        let value = percent
            .map(|v| format!("{}%", v.round() as i64))
            .unwrap_or_else(|| "--%".to_string());
        return format!("CX {value}");
    }

    "--%".to_string()
}

/// Determine usage level from session percentage.
/// Returns: 0 = low (green), 1 = medium (orange), 2 = high (red), -1 = unknown (gray)
pub(crate) fn usage_level(
    track_claude: bool,
    track_codex: bool,
    snapshot: Option<&UsageSnapshotBundle>,
) -> i8 {
    let claude = if track_claude {
        snapshot
            .and_then(|s| s.claude.as_ref())
            .and_then(view_claude)
            .map(|v| v.session_percent)
    } else {
        None
    };
    let codex = if track_codex {
        snapshot
            .and_then(|s| s.codex.as_ref())
            .and_then(view_codex)
            .map(|v| v.session_percent)
    } else {
        None
    };

    let session_percent = match (claude, codex) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };

    let Some(session_percent) = session_percent else {
        return -1;
    };

    if session_percent < 50.0 {
        0 // green
    } else if session_percent <= 70.0 {
        1 // orange
    } else {
        2 // red
    }
}
fn system_locale_tag() -> Option<String> {
    for key in ["LC_TIME", "LC_ALL", "LANG"] {
        if let Ok(value) = std::env::var(key) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

pub(crate) fn normalize_locale_tag(tag: &str) -> String {
    let tag = tag.trim();
    let tag = tag.split('.').next().unwrap_or(tag);
    let tag = tag.replace('-', "_");

    if tag.eq_ignore_ascii_case("c") {
        return "POSIX".to_string();
    }

    let (main, modifier) = match tag.split_once('@') {
        Some((main, modifier)) => (main, Some(modifier)),
        None => (tag.as_str(), None),
    };

    let mut parts = main.split('_');
    let language = parts.next().unwrap_or(main).to_ascii_lowercase();
    let territory = parts.next().map(|t| t.to_ascii_uppercase());

    let mut normalized = match territory {
        Some(territory) => format!("{language}_{territory}"),
        None => language,
    };
    if let Some(modifier) = modifier {
        normalized.push('@');
        normalized.push_str(modifier);
    }

    normalized
}

fn system_locale() -> Locale {
    let tag = system_locale_tag().unwrap_or_else(|| "en_US".to_string());
    let tag = normalize_locale_tag(&tag);
    tag.parse().unwrap_or(Locale::en_US)
}

pub(crate) fn strip_seconds_from_time_string(input: &str) -> String {
    let mut out = Vec::new();
    for token in input.split_whitespace() {
        let mut replaced = token.to_string();
        if token.matches(':').count() >= 2 {
            if let Some((before_last, last)) = token.rsplit_once(':') {
                if last.len() == 2 && last.chars().all(|c| c.is_ascii_digit()) {
                    replaced = before_last.to_string();
                }
            }
        }
        out.push(replaced);
    }
    out.join(" ")
}

fn clean_non_alnum_separators(input: &str) -> String {
    let mut out = String::new();
    let mut prev_was_sep = false;

    for c in input.chars() {
        let is_sep = !c.is_alphanumeric();
        if is_sep {
            if out.is_empty() {
                continue;
            }
            if prev_was_sep {
                continue;
            }
            out.push(c);
            prev_was_sep = true;
        } else {
            out.push(c);
            prev_was_sep = false;
        }
    }

    while out.chars().last().is_some_and(|c| !c.is_alphanumeric()) {
        out.pop();
    }

    out.trim().to_string()
}

pub(crate) fn strip_year_from_date_string(input: &str) -> String {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum SegmentKind {
        Alnum,
        Sep,
    }

    #[derive(Debug, Clone)]
    struct Segment {
        kind: SegmentKind,
        text: String,
    }

    let mut segments: Vec<Segment> = Vec::new();
    let mut cur_kind: Option<SegmentKind> = None;
    let mut cur_text = String::new();

    for c in input.chars() {
        let kind = if c.is_alphanumeric() {
            SegmentKind::Alnum
        } else {
            SegmentKind::Sep
        };

        if cur_kind == Some(kind) {
            cur_text.push(c);
        } else {
            if let Some(k) = cur_kind.take() {
                segments.push(Segment {
                    kind: k,
                    text: std::mem::take(&mut cur_text),
                });
            }
            cur_kind = Some(kind);
            cur_text.push(c);
        }
    }
    if let Some(k) = cur_kind {
        segments.push(Segment {
            kind: k,
            text: cur_text,
        });
    }

    let alnum_indices: Vec<usize> = segments
        .iter()
        .enumerate()
        .filter_map(|(idx, seg)| (seg.kind == SegmentKind::Alnum).then_some(idx))
        .collect();

    let mut removed_any = false;

    // Prefer removing a 4-digit ASCII year if present (common for many locales).
    if let Some(idx) = alnum_indices.iter().copied().find(|&idx| {
        let t = segments[idx].text.trim();
        t.len() == 4 && t.chars().all(|c| c.is_ascii_digit())
    }) {
        segments[idx].text.clear();
        removed_any = true;
    } else {
        // Fallback: remove a 2-digit year for common numeric short-date patterns (e.g., 01/28/26).
        let numeric_alnums: Vec<(usize, usize)> = alnum_indices
            .iter()
            .copied()
            .filter_map(|idx| {
                let t = segments[idx].text.trim();
                if t.is_empty() || !t.chars().all(|c| c.is_ascii_digit()) {
                    return None;
                }
                Some((idx, t.len()))
            })
            .collect();

        if numeric_alnums.len() == 3 {
            let (last_idx, last_len) = numeric_alnums[numeric_alnums.len() - 1];
            let first_len = numeric_alnums[0].1;
            let second_len = numeric_alnums[1].1;
            if last_len == 2 && first_len <= 2 && second_len <= 2 {
                segments[last_idx].text.clear();
                removed_any = true;
            }
        }
    }

    let rebuilt: String = segments
        .into_iter()
        .filter(|seg| !seg.text.trim().is_empty())
        .map(|seg| seg.text)
        .collect();

    if removed_any {
        clean_non_alnum_separators(&rebuilt)
    } else {
        clean_non_alnum_separators(input)
    }
}

#[allow(dead_code)]
pub(crate) fn format_time_short(iso: &str) -> Option<String> {
    // Locale + TZ rules:
    // - Time zone conversion uses `chrono::Local` (the OS-configured local time zone, incl. DST).
    // - Locale detection uses `LC_TIME` → `LC_ALL` → `LANG`.
    // - Formatting uses `chrono` `unstable-locales` (pure-rust-locales) for locale patterns.
    let dt: DateTime<FixedOffset> = DateTime::parse_from_rfc3339(iso).ok()?;
    let locale = system_locale();
    let formatted = dt
        .with_timezone(&Local)
        .format_localized("%X", locale)
        .to_string();
    Some(strip_seconds_from_time_string(&formatted))
}

pub(crate) fn format_reset_at_short(iso: &str) -> Option<String> {
    // Locale + TZ rules:
    // - Time zone conversion uses `chrono::Local` (the OS-configured local time zone, incl. DST).
    // - Locale detection uses `LC_TIME` → `LC_ALL` → `LANG`.
    // - Formatting uses `chrono` `unstable-locales` (pure-rust-locales) for locale patterns.
    let dt: DateTime<FixedOffset> = DateTime::parse_from_rfc3339(iso).ok()?;
    let locale = system_locale();
    let local = dt.with_timezone(&Local);

    let date_full = local.format_localized("%x", locale).to_string();
    let date = strip_year_from_date_string(&date_full);

    let time_full = local.format_localized("%X", locale).to_string();
    let time = strip_seconds_from_time_string(&time_full);

    if date.trim().is_empty() {
        Some(time)
    } else if time.trim().is_empty() {
        Some(date)
    } else {
        Some(format!("{date} {time}"))
    }
}

pub(crate) fn format_datetime_full(iso: &str) -> String {
    let Ok(dt) = DateTime::parse_from_rfc3339(iso) else {
        return iso.to_string();
    };
    let local = dt.with_timezone(&Local);
    let locale = system_locale();
    let date = local.format_localized("%x", locale).to_string();
    let time = local.format_localized("%X", locale).to_string();
    format!("{date}, {time}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ClaudeUsageSnapshot, CodexUsageSnapshot};

    fn sample_rfc3339_utc() -> &'static str {
        "2026-01-06T22:59:31Z"
    }

    #[test]
    fn format_time_short_strips_seconds_for_common_time_patterns() {
        assert_eq!(strip_seconds_from_time_string("06:54:32 AM"), "06:54 AM");
        assert_eq!(strip_seconds_from_time_string("18:54:32"), "18:54");
        assert_eq!(strip_seconds_from_time_string("06:54 AM"), "06:54 AM");
    }

    #[test]
    fn format_time_short_and_datetime_full_do_not_crash() {
        assert!(format_time_short(sample_rfc3339_utc()).is_some());
        let formatted = format_datetime_full(sample_rfc3339_utc());
        assert!(formatted.contains(", "));
    }

    #[test]
    fn format_datetime_full_falls_back_to_raw_input_on_parse_error() {
        let input = "not-a-datetime";
        assert_eq!(format_datetime_full(input), input);
    }

    #[test]
    fn normalize_locale_tag_handles_common_env_values() {
        assert_eq!(normalize_locale_tag("pt_BR.UTF-8"), "pt_BR");
        assert_eq!(normalize_locale_tag("de-de.UTF-8"), "de_DE");
        assert_eq!(normalize_locale_tag("C.UTF-8"), "POSIX");
    }

    #[test]
    fn strip_year_from_date_string_removes_yyyy_at_end() {
        assert_eq!(strip_year_from_date_string("28/01/2026"), "28/01");
    }

    #[test]
    fn strip_year_from_date_string_removes_yyyy_at_start() {
        assert_eq!(strip_year_from_date_string("2026/01/28"), "01/28");
    }

    #[test]
    fn strip_year_from_date_string_trims_separators() {
        assert_eq!(strip_year_from_date_string("28.01.2026"), "28.01");
    }

    #[test]
    fn strip_year_from_date_string_no_year_is_noop() {
        assert_eq!(strip_year_from_date_string("28/01"), "28/01");
    }

    #[test]
    fn strip_year_from_date_string_handles_two_digit_year_at_end() {
        assert_eq!(strip_year_from_date_string("01/28/26"), "01/28");
    }

    #[test]
    fn format_reset_at_short_does_not_crash() {
        assert!(format_reset_at_short(sample_rfc3339_utc()).is_some());
    }

    fn make_claude_ok_bundle(session_percent: f64) -> UsageSnapshotBundle {
        UsageSnapshotBundle {
            claude: Some(ClaudeUsageSnapshot::Ok {
                organization_id: "org-123".to_string(),
                session_percent,
                session_resets_at: Some("2026-01-07T05:00:00Z".to_string()),
                weekly_percent: 30.0,
                weekly_resets_at: Some("2026-01-13T00:00:00Z".to_string()),
                models: vec![],
                last_updated_at: "2026-01-06T22:59:31Z".to_string(),
            }),
            codex: None,
        }
    }

    fn make_codex_ok_bundle(session_percent: f64) -> UsageSnapshotBundle {
        UsageSnapshotBundle {
            claude: None,
            codex: Some(CodexUsageSnapshot::Ok {
                session_percent,
                session_resets_at: Some("2026-01-07T05:00:00Z".to_string()),
                weekly_percent: 30.0,
                weekly_resets_at: Some("2026-01-13T00:00:00Z".to_string()),
                last_updated_at: "2026-01-06T22:59:31Z".to_string(),
            }),
        }
    }

    #[test]
    fn format_tray_title_shows_percentage() {
        let snapshot = make_claude_ok_bundle(25.0);
        let title = format_tray_title(true, false, Some(&snapshot));
        assert_eq!(title, "CL 25%");
    }

    #[test]
    fn format_tray_title_rounds_49_point_9_to_50() {
        let snapshot = make_claude_ok_bundle(49.9);
        let title = format_tray_title(true, false, Some(&snapshot));
        assert_eq!(title, "CL 50%");
    }

    #[test]
    fn format_tray_title_shows_100_percent() {
        let snapshot = make_claude_ok_bundle(100.0);
        let title = format_tray_title(true, false, Some(&snapshot));
        assert_eq!(title, "CL 100%");
    }

    #[test]
    fn format_tray_title_shows_placeholder_for_none() {
        let title = format_tray_title(true, false, None);
        assert_eq!(title, "CL --%");
    }

    #[test]
    fn format_tray_title_shows_placeholder_for_unauthorized() {
        let snapshot = UsageSnapshotBundle {
            claude: Some(ClaudeUsageSnapshot::Unauthorized {
                organization_id: None,
                error_message: Some("Invalid session".to_string()),
                last_updated_at: "2026-01-06T22:59:31Z".to_string(),
            }),
            codex: None,
        };
        let title = format_tray_title(true, false, Some(&snapshot));
        assert_eq!(title, "CL --%");
    }

    #[test]
    fn format_tray_title_shows_placeholder_for_missing_key() {
        let snapshot = UsageSnapshotBundle {
            claude: Some(ClaudeUsageSnapshot::MissingKey {
                organization_id: None,
                error_message: None,
                last_updated_at: "2026-01-06T22:59:31Z".to_string(),
            }),
            codex: None,
        };
        let title = format_tray_title(true, false, Some(&snapshot));
        assert_eq!(title, "CL --%");
    }

    #[test]
    fn format_tray_title_shows_placeholder_for_rate_limited() {
        let snapshot = UsageSnapshotBundle {
            claude: Some(ClaudeUsageSnapshot::RateLimited {
                organization_id: None,
                error_message: Some("Too many requests".to_string()),
                last_updated_at: "2026-01-06T22:59:31Z".to_string(),
            }),
            codex: None,
        };
        let title = format_tray_title(true, false, Some(&snapshot));
        assert_eq!(title, "CL --%");
    }

    #[test]
    fn format_tray_title_shows_placeholder_for_error() {
        let snapshot = UsageSnapshotBundle {
            claude: Some(ClaudeUsageSnapshot::Error {
                organization_id: None,
                error_message: Some("Network error".to_string()),
                last_updated_at: "2026-01-06T22:59:31Z".to_string(),
            }),
            codex: None,
        };
        let title = format_tray_title(true, false, Some(&snapshot));
        assert_eq!(title, "CL --%");
    }

    #[test]
    fn format_tray_title_rounds_percentage_correctly() {
        let snapshot = make_claude_ok_bundle(75.7);
        let title = format_tray_title(true, false, Some(&snapshot));
        assert!(title.contains("76%"), "75.7 should round to 76");

        let snapshot = make_claude_ok_bundle(75.4);
        let title = format_tray_title(true, false, Some(&snapshot));
        assert!(title.contains("75%"), "75.4 should round to 75");
    }

    #[test]
    fn usage_level_returns_green_below_50() {
        assert_eq!(
            usage_level(true, false, Some(&make_claude_ok_bundle(0.0))),
            0
        );
        assert_eq!(
            usage_level(true, false, Some(&make_claude_ok_bundle(25.0))),
            0
        );
        assert_eq!(
            usage_level(true, false, Some(&make_claude_ok_bundle(49.9))),
            0
        );
    }

    #[test]
    fn usage_level_returns_orange_between_50_and_70() {
        assert_eq!(
            usage_level(true, false, Some(&make_claude_ok_bundle(50.0))),
            1
        );
        assert_eq!(
            usage_level(true, false, Some(&make_claude_ok_bundle(65.0))),
            1
        );
        assert_eq!(
            usage_level(true, false, Some(&make_claude_ok_bundle(70.0))),
            1
        );
    }

    #[test]
    fn usage_level_returns_red_above_70() {
        assert_eq!(
            usage_level(true, false, Some(&make_claude_ok_bundle(70.1))),
            2
        );
        assert_eq!(
            usage_level(true, false, Some(&make_claude_ok_bundle(90.0))),
            2
        );
        assert_eq!(
            usage_level(true, false, Some(&make_claude_ok_bundle(100.0))),
            2
        );
    }

    #[test]
    fn usage_level_returns_unknown_for_error_states() {
        assert_eq!(usage_level(true, false, None), -1);
        let error = UsageSnapshotBundle {
            claude: Some(ClaudeUsageSnapshot::Error {
                organization_id: None,
                error_message: None,
                last_updated_at: "2026-01-06T22:59:31Z".to_string(),
            }),
            codex: None,
        };
        assert_eq!(usage_level(true, false, Some(&error)), -1);
    }

    #[test]
    fn format_tray_title_in_dual_mode_shows_both_providers() {
        let snapshot = UsageSnapshotBundle {
            claude: make_claude_ok_bundle(25.0).claude,
            codex: make_codex_ok_bundle(10.0).codex,
        };
        let title = format_tray_title(true, true, Some(&snapshot));
        assert_eq!(title, "CL 25% · CX 10%");
    }

    #[test]
    fn usage_level_in_dual_mode_uses_max_severity() {
        let snapshot = UsageSnapshotBundle {
            claude: make_claude_ok_bundle(10.0).claude,
            codex: make_codex_ok_bundle(95.0).codex,
        };
        assert_eq!(usage_level(true, true, Some(&snapshot)), 2);
    }
}
