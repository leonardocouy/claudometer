use crate::redact::redact_session_key;
use crate::types::{ClaudeOrganization, ClaudeUsageSnapshot};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, COOKIE, ORIGIN, REFERER, USER_AGENT};
use serde_json::Value;
use thiserror::Error;
use time::OffsetDateTime;

const BASE_URL: &str = "https://claude.ai/api";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeWebErrorStatus {
  Unauthorized,
  RateLimited,
  Error,
}

#[derive(Debug, Error)]
pub enum ClaudeError {
  #[error("network error")]
  Network(#[from] reqwest::Error),
  #[error("invalid json")]
  Json(#[from] serde_json::Error),
}

fn now_iso() -> String {
  OffsetDateTime::now_utc()
    .format(&time::format_description::well_known::Rfc3339)
    .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn build_headers(session_key: &str) -> HeaderMap {
  let mut headers = HeaderMap::new();
  headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
  if let Ok(cookie) = HeaderValue::from_str(&format!("sessionKey={session_key}")) {
    headers.insert(COOKIE, cookie);
  }
  headers.insert(
    USER_AGENT,
    HeaderValue::from_static(
      "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
    ),
  );
  headers.insert(ORIGIN, HeaderValue::from_static("https://claude.ai"));
  headers.insert(REFERER, HeaderValue::from_static("https://claude.ai/"));
  headers
}

fn map_http_status(status_code: u16) -> ClaudeWebErrorStatus {
  match status_code {
    401 | 403 => ClaudeWebErrorStatus::Unauthorized,
    429 => ClaudeWebErrorStatus::RateLimited,
    _ => ClaudeWebErrorStatus::Error,
  }
}

fn clamp_percent(value: f64) -> f64 {
  if value.is_nan() {
    return 0.0;
  }
  value.clamp(0.0, 100.0)
}

fn parse_utilization_percent(value: &Value) -> f64 {
  match value {
    Value::Number(n) => n.as_f64().map(clamp_percent).unwrap_or(0.0),
    Value::String(s) => s.trim().parse::<f64>().map(clamp_percent).unwrap_or(0.0),
    _ => 0.0,
  }
}

fn read_string(value: Option<&Value>) -> Option<String> {
  let s = value?.as_str()?.trim();
  if s.is_empty() {
    None
  } else {
    Some(s.to_string())
  }
}

fn title_case(value: &str) -> String {
  value
    .split(|c: char| c == '_' || c.is_whitespace())
    .filter(|p| !p.is_empty())
    .map(|part| {
      let mut chars = part.chars();
      match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
      }
    })
    .collect::<Vec<_>>()
    .join(" ")
}

fn read_model_weekly_usage(root: &serde_json::Map<String, Value>) -> (f64, Option<String>, Option<String>) {
  let preferred = ["seven_day_sonnet", "seven_day_opus"];
  for key in preferred {
    if let Some(Value::Object(period)) = root.get(key) {
      let percent = parse_utilization_percent(period.get("utilization").unwrap_or(&Value::Null));
      let resets_at = read_string(period.get("resets_at"));
      let name = title_case(key.trim_start_matches("seven_day_"));
      return (percent, Some(name), resets_at);
    }
  }

  for (key, value) in root.iter() {
    if !key.starts_with("seven_day_") || key == "seven_day" {
      continue;
    }
    let Some(period) = value.as_object() else {
      continue;
    };
    let percent = parse_utilization_percent(period.get("utilization").unwrap_or(&Value::Null));
    if percent == 0.0 {
      continue;
    }
    let resets_at = read_string(period.get("resets_at"));
    let name = title_case(key.trim_start_matches("seven_day_"));
    return (percent, Some(name), resets_at);
  }

  (0.0, None, None)
}

fn parse_usage_from_json(
  json: Value,
  organization_id: &str,
  last_updated_at: &str,
) -> ClaudeUsageSnapshot {
  let root = json.as_object().cloned().unwrap_or_default();
  let five_hour = root.get("five_hour").and_then(|v| v.as_object());
  let seven_day = root.get("seven_day").and_then(|v| v.as_object());

  let session_percent = five_hour
    .and_then(|o| o.get("utilization"))
    .map(parse_utilization_percent)
    .unwrap_or(0.0);
  let session_resets_at = five_hour.and_then(|o| read_string(o.get("resets_at")));

  let weekly_percent = seven_day
    .and_then(|o| o.get("utilization"))
    .map(parse_utilization_percent)
    .unwrap_or(0.0);
  let weekly_resets_at = seven_day.and_then(|o| read_string(o.get("resets_at")));

  let (model_weekly_percent, model_weekly_name, model_weekly_resets_at) =
    read_model_weekly_usage(&root);

  ClaudeUsageSnapshot::Ok {
    organization_id: organization_id.to_string(),
    session_percent,
    session_resets_at,
    weekly_percent,
    weekly_resets_at,
    model_weekly_percent,
    model_weekly_name,
    model_weekly_resets_at,
    last_updated_at: last_updated_at.to_string(),
  }
}

pub struct ClaudeApiClient {
  http: reqwest::Client,
}

impl ClaudeApiClient {
  pub fn new() -> Result<Self, ClaudeError> {
    Ok(Self {
      http: reqwest::Client::builder().build()?,
    })
  }

  pub async fn fetch_organizations_checked(
    &self,
    session_key: &str,
  ) -> Result<Vec<ClaudeOrganization>, ClaudeWebErrorStatus> {
    let url = format!("{BASE_URL}/organizations");
    let res = self
      .http
      .get(url)
      .headers(build_headers(session_key))
      .send()
      .await;

    let res = match res {
      Ok(r) => r,
      Err(_) => return Err(ClaudeWebErrorStatus::Error),
    };

    if !res.status().is_success() {
      return Err(map_http_status(res.status().as_u16()));
    }

    let json: Value = match res.json().await {
      Ok(v) => v,
      Err(_) => return Err(ClaudeWebErrorStatus::Error),
    };
    let Some(arr) = json.as_array() else {
      return Ok(vec![]);
    };

    let mut out = Vec::new();
    for entry in arr {
      let Some(obj) = entry.as_object() else {
        continue;
      };
      let Some(uuid) = obj.get("uuid").and_then(|v| v.as_str()) else {
        continue;
      };
      let uuid = uuid.trim();
      if uuid.is_empty() {
        continue;
      }
      let name = obj.get("name").and_then(|v| v.as_str()).map(|s| s.trim()).filter(|s| !s.is_empty()).map(|s| s.to_string());
      out.push(ClaudeOrganization {
        id: uuid.to_string(),
        name,
      });
    }
    Ok(out)
  }

  pub async fn fetch_usage_snapshot(
    &self,
    session_key: &str,
    organization_id: &str,
  ) -> ClaudeUsageSnapshot {
    let last_updated_at = now_iso();
    let url = format!(
      "{BASE_URL}/organizations/{}/usage",
      urlencoding::encode(organization_id)
    );

    let res = self
      .http
      .get(url)
      .headers(build_headers(session_key))
      .send()
      .await;

    let res = match res {
      Ok(r) => r,
      Err(e) => {
        let msg = redact_session_key(&e.to_string()).to_string();
        return ClaudeUsageSnapshot::Error {
          organization_id: Some(organization_id.to_string()),
          last_updated_at,
          error_message: Some(msg),
        };
      }
    };

    if !res.status().is_success() {
      let status = map_http_status(res.status().as_u16());
      let msg = format!("Claude API error ({})", res.status().as_u16());
      return match status {
        ClaudeWebErrorStatus::Unauthorized => ClaudeUsageSnapshot::Unauthorized {
          organization_id: Some(organization_id.to_string()),
          last_updated_at,
          error_message: Some(msg),
        },
        ClaudeWebErrorStatus::RateLimited => ClaudeUsageSnapshot::RateLimited {
          organization_id: Some(organization_id.to_string()),
          last_updated_at,
          error_message: Some(msg),
        },
        ClaudeWebErrorStatus::Error => ClaudeUsageSnapshot::Error {
          organization_id: Some(organization_id.to_string()),
          last_updated_at,
          error_message: Some(msg),
        },
      };
    }

    let text = res.text().await;
    let text = match text {
      Ok(t) => t,
      Err(e) => {
        let msg = redact_session_key(&e.to_string()).to_string();
        return ClaudeUsageSnapshot::Error {
          organization_id: Some(organization_id.to_string()),
          last_updated_at,
          error_message: Some(msg),
        };
      }
    };

    match serde_json::from_str::<Value>(&text) {
      Ok(json) => parse_usage_from_json(json, organization_id, &last_updated_at),
      Err(e) => ClaudeUsageSnapshot::Error {
        organization_id: Some(organization_id.to_string()),
        last_updated_at,
        error_message: Some(redact_session_key(&e.to_string()).to_string()),
      },
    }
  }
}
