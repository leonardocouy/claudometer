const NEAR_LIMIT_THRESHOLD_PERCENT: f64 = 90.0;
const UNKNOWN_PERIOD_ID: &str = "unknown";

#[derive(Debug, Clone, PartialEq)]
pub struct NearLimitAlertDecision {
  pub notify_session: bool,
  pub notify_weekly: bool,
  pub session_period_id: Option<String>,
  pub weekly_period_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsageResetDecision {
  pub notify_session_reset: bool,
  pub notify_weekly_reset: bool,
  pub session_reset_period_id: Option<String>,
  pub weekly_reset_period_id: Option<String>,
}

fn normalize_period_id(resets_at: Option<&str>) -> String {
  let trimmed = resets_at.unwrap_or("").trim();
  if trimmed.is_empty() {
    UNKNOWN_PERIOD_ID.to_string()
  } else {
    trimmed.to_string()
  }
}

fn should_notify_near_limit(params: ShouldNotifyNearLimitParams<'_>) -> bool {
  if params.current_percent < NEAR_LIMIT_THRESHOLD_PERCENT {
    return false;
  }
  if params.last_notified_period_id == Some(params.current_period_id) {
    return false;
  }
  match params.previous_percent {
    None => true,
    Some(prev) => prev < NEAR_LIMIT_THRESHOLD_PERCENT,
  }
}

struct ShouldNotifyNearLimitParams<'a> {
  current_percent: f64,
  previous_percent: Option<f64>,
  current_period_id: &'a str,
  last_notified_period_id: Option<&'a str>,
}

pub fn decide_near_limit_alerts(params: DecideNearLimitAlertsParams<'_>) -> NearLimitAlertDecision {
  let session_period_id = normalize_period_id(params.current_session_resets_at);
  let weekly_period_id = normalize_period_id(params.current_weekly_resets_at);

  let notify_session = should_notify_near_limit(ShouldNotifyNearLimitParams {
    current_percent: params.current_session_percent,
    previous_percent: params.previous_session_percent,
    current_period_id: &session_period_id,
    last_notified_period_id: params.last_notified_session_period_id,
  });

  let notify_weekly = should_notify_near_limit(ShouldNotifyNearLimitParams {
    current_percent: params.current_weekly_percent,
    previous_percent: params.previous_weekly_percent,
    current_period_id: &weekly_period_id,
    last_notified_period_id: params.last_notified_weekly_period_id,
  });

  NearLimitAlertDecision {
    notify_session,
    notify_weekly,
    session_period_id: notify_session.then(|| session_period_id),
    weekly_period_id: notify_weekly.then(|| weekly_period_id),
  }
}

pub struct DecideNearLimitAlertsParams<'a> {
  pub current_session_percent: f64,
  pub current_weekly_percent: f64,
  pub current_session_resets_at: Option<&'a str>,
  pub current_weekly_resets_at: Option<&'a str>,
  pub previous_session_percent: Option<f64>,
  pub previous_weekly_percent: Option<f64>,
  pub last_notified_session_period_id: Option<&'a str>,
  pub last_notified_weekly_period_id: Option<&'a str>,
}

fn should_notify_reset(params: ShouldNotifyResetParams<'_>) -> bool {
  if params.current_period_id.is_empty() || params.current_period_id == UNKNOWN_PERIOD_ID {
    return false;
  }
  let Some(last_seen) = params.last_seen_period_id else {
    return false;
  };
  if params.current_period_id == last_seen {
    return false;
  }
  if params.last_notified_reset_period_id == Some(params.current_period_id) {
    return false;
  }
  true
}

struct ShouldNotifyResetParams<'a> {
  current_period_id: &'a str,
  last_seen_period_id: Option<&'a str>,
  last_notified_reset_period_id: Option<&'a str>,
}

pub fn decide_usage_resets(params: DecideUsageResetsParams<'_>) -> UsageResetDecision {
  let session_period_id = normalize_period_id(params.current_session_resets_at);
  let weekly_period_id = normalize_period_id(params.current_weekly_resets_at);

  let notify_session_reset = should_notify_reset(ShouldNotifyResetParams {
    current_period_id: &session_period_id,
    last_seen_period_id: params.last_seen_session_period_id,
    last_notified_reset_period_id: params.last_notified_session_reset_period_id,
  });

  let notify_weekly_reset = should_notify_reset(ShouldNotifyResetParams {
    current_period_id: &weekly_period_id,
    last_seen_period_id: params.last_seen_weekly_period_id,
    last_notified_reset_period_id: params.last_notified_weekly_reset_period_id,
  });

  UsageResetDecision {
    notify_session_reset,
    notify_weekly_reset,
    session_reset_period_id: notify_session_reset.then(|| session_period_id),
    weekly_reset_period_id: notify_weekly_reset.then(|| weekly_period_id),
  }
}

pub struct DecideUsageResetsParams<'a> {
  pub current_session_resets_at: Option<&'a str>,
  pub current_weekly_resets_at: Option<&'a str>,
  pub last_seen_session_period_id: Option<&'a str>,
  pub last_seen_weekly_period_id: Option<&'a str>,
  pub last_notified_session_reset_period_id: Option<&'a str>,
  pub last_notified_weekly_reset_period_id: Option<&'a str>,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn near_limit_does_not_notify_below_threshold() {
    let result = decide_near_limit_alerts(DecideNearLimitAlertsParams {
      current_session_percent: 89.9,
      current_weekly_percent: 0.0,
      current_session_resets_at: Some("2026-01-01T05:00:00.000Z"),
      current_weekly_resets_at: Some("2026-01-08T00:00:00.000Z"),
      previous_session_percent: None,
      previous_weekly_percent: None,
      last_notified_session_period_id: None,
      last_notified_weekly_period_id: None,
    });
    assert!(!result.notify_session);
    assert!(!result.notify_weekly);
  }

  #[test]
  fn near_limit_notifies_once_on_first_observation() {
    let result = decide_near_limit_alerts(DecideNearLimitAlertsParams {
      current_session_percent: 90.0,
      current_weekly_percent: 95.0,
      current_session_resets_at: Some("2026-01-01T05:00:00.000Z"),
      current_weekly_resets_at: Some("2026-01-08T00:00:00.000Z"),
      previous_session_percent: None,
      previous_weekly_percent: None,
      last_notified_session_period_id: Some(""),
      last_notified_weekly_period_id: Some(""),
    });
    assert!(result.notify_session);
    assert_eq!(result.session_period_id.as_deref(), Some("2026-01-01T05:00:00.000Z"));
    assert!(result.notify_weekly);
    assert_eq!(result.weekly_period_id.as_deref(), Some("2026-01-08T00:00:00.000Z"));
  }

  #[test]
  fn near_limit_does_not_notify_again_for_same_period_id() {
    let result = decide_near_limit_alerts(DecideNearLimitAlertsParams {
      current_session_percent: 99.0,
      current_weekly_percent: 99.0,
      current_session_resets_at: Some("2026-01-01T05:00:00.000Z"),
      current_weekly_resets_at: Some("2026-01-08T00:00:00.000Z"),
      previous_session_percent: Some(95.0),
      previous_weekly_percent: Some(95.0),
      last_notified_session_period_id: Some("2026-01-01T05:00:00.000Z"),
      last_notified_weekly_period_id: Some("2026-01-08T00:00:00.000Z"),
    });
    assert!(!result.notify_session);
    assert!(!result.notify_weekly);
  }

  #[test]
  fn near_limit_allows_notification_again_when_period_id_changes() {
    let result = decide_near_limit_alerts(DecideNearLimitAlertsParams {
      current_session_percent: 92.0,
      current_weekly_percent: 92.0,
      current_session_resets_at: Some("2026-01-01T10:00:00.000Z"),
      current_weekly_resets_at: Some("2026-01-15T00:00:00.000Z"),
      previous_session_percent: Some(10.0),
      previous_weekly_percent: Some(10.0),
      last_notified_session_period_id: Some("2026-01-01T05:00:00.000Z"),
      last_notified_weekly_period_id: Some("2026-01-08T00:00:00.000Z"),
    });
    assert!(result.notify_session);
    assert_eq!(result.session_period_id.as_deref(), Some("2026-01-01T10:00:00.000Z"));
    assert!(result.notify_weekly);
    assert_eq!(result.weekly_period_id.as_deref(), Some("2026-01-15T00:00:00.000Z"));
  }

  #[test]
  fn near_limit_uses_unknown_period_id_when_resets_at_missing() {
    let first = decide_near_limit_alerts(DecideNearLimitAlertsParams {
      current_session_percent: 90.0,
      current_weekly_percent: 90.0,
      current_session_resets_at: None,
      current_weekly_resets_at: None,
      previous_session_percent: None,
      previous_weekly_percent: None,
      last_notified_session_period_id: None,
      last_notified_weekly_period_id: None,
    });
    assert!(first.notify_session);
    assert_eq!(first.session_period_id.as_deref(), Some("unknown"));
    assert!(first.notify_weekly);
    assert_eq!(first.weekly_period_id.as_deref(), Some("unknown"));

    let second = decide_near_limit_alerts(DecideNearLimitAlertsParams {
      current_session_percent: 95.0,
      current_weekly_percent: 95.0,
      current_session_resets_at: None,
      current_weekly_resets_at: None,
      previous_session_percent: Some(90.0),
      previous_weekly_percent: Some(90.0),
      last_notified_session_period_id: Some("unknown"),
      last_notified_weekly_period_id: Some("unknown"),
    });
    assert!(!second.notify_session);
    assert!(!second.notify_weekly);
  }

  #[test]
  fn resets_do_not_notify_on_first_observation() {
    let result = decide_usage_resets(DecideUsageResetsParams {
      current_session_resets_at: Some("2026-01-01T05:00:00.000Z"),
      current_weekly_resets_at: Some("2026-01-08T00:00:00.000Z"),
      last_seen_session_period_id: None,
      last_seen_weekly_period_id: None,
      last_notified_session_reset_period_id: None,
      last_notified_weekly_reset_period_id: None,
    });
    assert!(!result.notify_session_reset);
    assert!(!result.notify_weekly_reset);
  }

  #[test]
  fn resets_do_not_notify_when_period_has_not_changed() {
    let result = decide_usage_resets(DecideUsageResetsParams {
      current_session_resets_at: Some("2026-01-01T05:00:00.000Z"),
      current_weekly_resets_at: Some("2026-01-08T00:00:00.000Z"),
      last_seen_session_period_id: Some("2026-01-01T05:00:00.000Z"),
      last_seen_weekly_period_id: Some("2026-01-08T00:00:00.000Z"),
      last_notified_session_reset_period_id: None,
      last_notified_weekly_reset_period_id: None,
    });
    assert!(!result.notify_session_reset);
    assert!(!result.notify_weekly_reset);
  }

  #[test]
  fn resets_notify_when_period_changes() {
    let result = decide_usage_resets(DecideUsageResetsParams {
      current_session_resets_at: Some("2026-01-01T10:00:00.000Z"),
      current_weekly_resets_at: Some("2026-01-15T00:00:00.000Z"),
      last_seen_session_period_id: Some("2026-01-01T05:00:00.000Z"),
      last_seen_weekly_period_id: Some("2026-01-08T00:00:00.000Z"),
      last_notified_session_reset_period_id: None,
      last_notified_weekly_reset_period_id: None,
    });
    assert!(result.notify_session_reset);
    assert_eq!(
      result.session_reset_period_id.as_deref(),
      Some("2026-01-01T10:00:00.000Z")
    );
    assert!(result.notify_weekly_reset);
    assert_eq!(
      result.weekly_reset_period_id.as_deref(),
      Some("2026-01-15T00:00:00.000Z")
    );
  }

  #[test]
  fn resets_do_not_notify_again_for_same_new_period() {
    let result = decide_usage_resets(DecideUsageResetsParams {
      current_session_resets_at: Some("2026-01-01T10:00:00.000Z"),
      current_weekly_resets_at: Some("2026-01-15T00:00:00.000Z"),
      last_seen_session_period_id: Some("2026-01-01T05:00:00.000Z"),
      last_seen_weekly_period_id: Some("2026-01-08T00:00:00.000Z"),
      last_notified_session_reset_period_id: Some("2026-01-01T10:00:00.000Z"),
      last_notified_weekly_reset_period_id: Some("2026-01-15T00:00:00.000Z"),
    });
    assert!(!result.notify_session_reset);
    assert!(!result.notify_weekly_reset);
  }

  #[test]
  fn resets_do_not_notify_when_resets_at_missing_or_unknown() {
    let result = decide_usage_resets(DecideUsageResetsParams {
      current_session_resets_at: Some(""),
      current_weekly_resets_at: None,
      last_seen_session_period_id: Some("2026-01-01T05:00:00.000Z"),
      last_seen_weekly_period_id: Some("2026-01-08T00:00:00.000Z"),
      last_notified_session_reset_period_id: None,
      last_notified_weekly_reset_period_id: None,
    });
    assert!(!result.notify_session_reset);
    assert!(!result.notify_weekly_reset);
  }

  #[test]
  fn session_and_weekly_resets_are_independent() {
    let result = decide_usage_resets(DecideUsageResetsParams {
      current_session_resets_at: Some("2026-01-01T10:00:00.000Z"),
      current_weekly_resets_at: Some("2026-01-08T00:00:00.000Z"),
      last_seen_session_period_id: Some("2026-01-01T05:00:00.000Z"),
      last_seen_weekly_period_id: Some("2026-01-08T00:00:00.000Z"),
      last_notified_session_reset_period_id: None,
      last_notified_weekly_reset_period_id: None,
    });
    assert!(result.notify_session_reset);
    assert_eq!(
      result.session_reset_period_id.as_deref(),
      Some("2026-01-01T10:00:00.000Z")
    );
    assert!(!result.notify_weekly_reset);
  }
}
