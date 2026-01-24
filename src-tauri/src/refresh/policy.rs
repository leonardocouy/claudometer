use crate::types::{UsageSnapshotBundle, UsageStatus};

pub(crate) fn should_pause_polling(
    track_claude: bool,
    track_codex: bool,
    snapshot: &UsageSnapshotBundle,
) -> bool {
    let claude_paused = track_claude
        && snapshot.claude.as_ref().is_some_and(|s| {
            matches!(
                s.status(),
                UsageStatus::MissingKey | UsageStatus::Unauthorized
            )
        });
    let codex_paused = track_codex
        && snapshot.codex.as_ref().is_some_and(|s| {
            matches!(
                s.status(),
                UsageStatus::MissingKey | UsageStatus::Unauthorized
            )
        });

    if track_claude && track_codex {
        claude_paused && codex_paused
    } else {
        claude_paused || codex_paused
    }
}

fn compute_next_delay_ms_with_nanos(base_ms: u64, ratio: f64, nanos: i128) -> u64 {
    let frac = ((nanos % 1000) as f64) / 1000.0;
    let delta = (frac * 2.0 - 1.0) * (base_ms as f64 * ratio);
    ((base_ms as f64 + delta).max(1000.0)) as u64
}

pub(crate) fn compute_next_delay_ms(
    refresh_interval_seconds: u64,
    snapshot: &UsageSnapshotBundle,
) -> u64 {
    let base_seconds = refresh_interval_seconds.max(30);
    let configured_base_ms = base_seconds * 1000;

    let any_rate_limited = snapshot
        .claude
        .as_ref()
        .is_some_and(|s| s.status() == UsageStatus::RateLimited)
        || snapshot
            .codex
            .as_ref()
            .is_some_and(|s| s.status() == UsageStatus::RateLimited);

    let (base_ms, ratio) = if any_rate_limited {
        (5 * 60 * 1000, 0.2)
    } else {
        (configured_base_ms, 0.1)
    };

    let nanos = time::OffsetDateTime::now_utc().unix_timestamp_nanos();
    compute_next_delay_ms_with_nanos(base_ms, ratio, nanos)
}

pub(crate) fn compute_next_delay_for_latest(
    track_claude: bool,
    track_codex: bool,
    refresh_interval_seconds: u64,
    snapshot: Option<&UsageSnapshotBundle>,
) -> Option<u64> {
    let Some(snapshot) = snapshot else {
        return Some(60_000);
    };

    if should_pause_polling(track_claude, track_codex, snapshot) {
        None
    } else {
        Some(compute_next_delay_ms(refresh_interval_seconds, snapshot))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ClaudeUsageSnapshot, CodexUsageSnapshot};

    fn bundle_with_status(
        claude: Option<UsageStatus>,
        codex: Option<UsageStatus>,
    ) -> UsageSnapshotBundle {
        let claude = claude.map(|s| match s {
            UsageStatus::Ok => ClaudeUsageSnapshot::Ok {
                organization_id: "org".to_string(),
                session_percent: 10.0,
                session_resets_at: None,
                weekly_percent: 10.0,
                weekly_resets_at: None,
                models: vec![],
                last_updated_at: "2026-01-01T00:00:00Z".to_string(),
            },
            UsageStatus::Unauthorized => ClaudeUsageSnapshot::Unauthorized {
                organization_id: None,
                last_updated_at: "2026-01-01T00:00:00Z".to_string(),
                error_message: None,
            },
            UsageStatus::MissingKey => ClaudeUsageSnapshot::MissingKey {
                organization_id: None,
                last_updated_at: "2026-01-01T00:00:00Z".to_string(),
                error_message: None,
            },
            UsageStatus::RateLimited => ClaudeUsageSnapshot::RateLimited {
                organization_id: None,
                last_updated_at: "2026-01-01T00:00:00Z".to_string(),
                error_message: None,
            },
            UsageStatus::Error => ClaudeUsageSnapshot::Error {
                organization_id: None,
                last_updated_at: "2026-01-01T00:00:00Z".to_string(),
                error_message: None,
            },
        });

        let codex = codex.map(|s| match s {
            UsageStatus::Ok => CodexUsageSnapshot::Ok {
                session_percent: 10.0,
                session_resets_at: None,
                weekly_percent: 10.0,
                weekly_resets_at: None,
                last_updated_at: "2026-01-01T00:00:00Z".to_string(),
            },
            UsageStatus::Unauthorized => CodexUsageSnapshot::Unauthorized {
                last_updated_at: "2026-01-01T00:00:00Z".to_string(),
                error_message: None,
            },
            UsageStatus::MissingKey => CodexUsageSnapshot::MissingKey {
                last_updated_at: "2026-01-01T00:00:00Z".to_string(),
                error_message: None,
            },
            UsageStatus::RateLimited => CodexUsageSnapshot::RateLimited {
                last_updated_at: "2026-01-01T00:00:00Z".to_string(),
                error_message: None,
            },
            UsageStatus::Error => CodexUsageSnapshot::Error {
                last_updated_at: "2026-01-01T00:00:00Z".to_string(),
                error_message: None,
            },
        });

        UsageSnapshotBundle { claude, codex }
    }

    #[test]
    fn should_pause_polling_pauses_only_when_all_enabled_providers_blocked() {
        let snapshot = bundle_with_status(Some(UsageStatus::Unauthorized), Some(UsageStatus::Ok));
        assert!(!should_pause_polling(true, true, &snapshot));

        let snapshot = bundle_with_status(
            Some(UsageStatus::Unauthorized),
            Some(UsageStatus::MissingKey),
        );
        assert!(should_pause_polling(true, true, &snapshot));

        let snapshot = bundle_with_status(Some(UsageStatus::Unauthorized), None);
        assert!(should_pause_polling(true, false, &snapshot));
    }

    #[test]
    fn compute_next_delay_ms_with_nanos_is_bounded() {
        let base_ms = 60_000_u64;

        let slow = compute_next_delay_ms_with_nanos(base_ms, 0.1, 0);
        assert!(slow <= base_ms);

        let fast = compute_next_delay_ms_with_nanos(base_ms, 0.1, 999);
        assert!(fast >= base_ms);

        let min = compute_next_delay_ms_with_nanos(500, 0.1, 0);
        assert!(min >= 1000);
    }

    #[test]
    fn compute_next_delay_for_latest_returns_none_when_paused() {
        let snapshot = bundle_with_status(
            Some(UsageStatus::MissingKey),
            Some(UsageStatus::Unauthorized),
        );
        assert_eq!(
            compute_next_delay_for_latest(true, true, 60, Some(&snapshot)),
            None
        );
    }
}
