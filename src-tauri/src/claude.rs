use crate::redact::redact_session_key;
use crate::types::{ClaudeModelUsage, ClaudeOrganization, ClaudeUsageSnapshot};
use reqwest::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT, AUTHORIZATION, COOKIE, ORIGIN, REFERER, USER_AGENT,
};
use serde_json::Value;
use std::path::{Path, PathBuf};
use thiserror::Error;
use time::OffsetDateTime;

const BASE_URL: &str = "https://claude.ai/api";
const OAUTH_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_BETA_HEADER: &str = "oauth-2025-04-20";
const CLI_CREDENTIALS_RELATIVE_PATH: &str = ".claude/.credentials.json";

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

fn build_oauth_headers(access_token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    if let Ok(value) = HeaderValue::from_str(&format!("Bearer {access_token}")) {
        headers.insert(AUTHORIZATION, value);
    }
    headers.insert(
    USER_AGENT,
    HeaderValue::from_static(
      "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
    ),
  );
    headers.insert(
        HeaderName::from_static("anthropic-beta"),
        HeaderValue::from_static(OAUTH_BETA_HEADER),
    );
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

fn read_model_weekly_usages(root: &serde_json::Map<String, Value>) -> Vec<ClaudeModelUsage> {
    let preferred = ["seven_day_sonnet", "seven_day_opus"];
    let mut out: Vec<ClaudeModelUsage> = Vec::new();
    for key in preferred {
        if let Some(Value::Object(period)) = root.get(key) {
            let percent =
                parse_utilization_percent(period.get("utilization").unwrap_or(&Value::Null));
            let resets_at = read_string(period.get("resets_at"));
            let name = title_case(key.trim_start_matches("seven_day_"));
            out.push(ClaudeModelUsage {
                name,
                percent,
                resets_at,
            });
        }
    }

    for (key, value) in root.iter() {
        if !key.starts_with("seven_day_") || key == "seven_day" {
            continue;
        }
        if preferred.contains(&key.as_str()) {
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
        out.push(ClaudeModelUsage {
            name,
            percent,
            resets_at,
        });
    }

    out
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

    let models = read_model_weekly_usages(&root);

    ClaudeUsageSnapshot::Ok {
        organization_id: organization_id.to_string(),
        session_percent,
        session_resets_at,
        weekly_percent,
        weekly_resets_at,
        models,
        last_updated_at: last_updated_at.to_string(),
    }
}

pub struct ClaudeApiClient {
    http: reqwest::Client,
}

impl ClaudeApiClient {
    pub fn new() -> Result<Self, ClaudeError> {
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(40))
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()?,
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
            let name = obj
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
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

    pub async fn fetch_oauth_usage_snapshot(&self, access_token: &str) -> ClaudeUsageSnapshot {
        let last_updated_at = now_iso();

        let res = self
            .http
            .get(OAUTH_USAGE_URL)
            .headers(build_oauth_headers(access_token))
            .send()
            .await;

        let res = match res {
            Ok(r) => r,
            Err(_) => {
                return ClaudeUsageSnapshot::Error {
                    organization_id: Some("oauth".to_string()),
                    last_updated_at,
                    error_message: Some("Network error while fetching OAuth usage.".to_string()),
                };
            }
        };

        if !res.status().is_success() {
            let status = map_http_status(res.status().as_u16());
            let msg = match status {
                ClaudeWebErrorStatus::Unauthorized => {
                    "OAuth usage is unauthorized. Re-authenticate (run `claude login`)."
                }
                ClaudeWebErrorStatus::RateLimited => "OAuth usage is rate limited.",
                ClaudeWebErrorStatus::Error => "OAuth usage request failed.",
            };
            return match status {
                ClaudeWebErrorStatus::Unauthorized => ClaudeUsageSnapshot::Unauthorized {
                    organization_id: Some("oauth".to_string()),
                    last_updated_at,
                    error_message: Some(msg.to_string()),
                },
                ClaudeWebErrorStatus::RateLimited => ClaudeUsageSnapshot::RateLimited {
                    organization_id: Some("oauth".to_string()),
                    last_updated_at,
                    error_message: Some(msg.to_string()),
                },
                ClaudeWebErrorStatus::Error => ClaudeUsageSnapshot::Error {
                    organization_id: Some("oauth".to_string()),
                    last_updated_at,
                    error_message: Some(msg.to_string()),
                },
            };
        }

        let text = match res.text().await {
            Ok(t) => t,
            Err(_) => {
                return ClaudeUsageSnapshot::Error {
                    organization_id: Some("oauth".to_string()),
                    last_updated_at,
                    error_message: Some("Failed to read OAuth usage response.".to_string()),
                };
            }
        };

        match serde_json::from_str::<Value>(&text) {
            Ok(json) => parse_usage_from_json(json, "oauth", &last_updated_at),
            Err(_) => ClaudeUsageSnapshot::Error {
                organization_id: Some("oauth".to_string()),
                last_updated_at,
                error_message: Some("Invalid JSON returned by OAuth usage endpoint.".to_string()),
            },
        }
    }
}

#[derive(Debug, Error)]
pub enum CliCredentialsError {
    #[error("HOME is not set")]
    HomeMissing,
    #[error("credentials file missing")]
    MissingFile,
    #[error("invalid credentials json")]
    InvalidJson,
    #[error("missing access token")]
    MissingAccessToken,
}

fn credentials_path() -> Result<PathBuf, CliCredentialsError> {
    let home = std::env::var_os("HOME").ok_or(CliCredentialsError::HomeMissing)?;
    Ok(PathBuf::from(home).join(CLI_CREDENTIALS_RELATIVE_PATH))
}

pub fn read_cli_oauth_access_token() -> Result<String, CliCredentialsError> {
    let path = credentials_path()?;
    read_cli_oauth_access_token_from_path(&path)
}

pub(crate) fn read_cli_oauth_access_token_from_path(
    path: &Path,
) -> Result<String, CliCredentialsError> {
    let contents = std::fs::read_to_string(path).map_err(|_| CliCredentialsError::MissingFile)?;
    let json: Value =
        serde_json::from_str(&contents).map_err(|_| CliCredentialsError::InvalidJson)?;
    extract_cli_oauth_access_token(&json).ok_or(CliCredentialsError::MissingAccessToken)
}

fn extract_cli_oauth_access_token(json: &Value) -> Option<String> {
    let root = json.as_object()?;
    let oauth = root.get("claudeAiOauth")?.as_object()?;
    let token = oauth.get("accessToken")?.as_str()?.trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_access_token() {
        let json: Value = serde_json::json!({
          "claudeAiOauth": {
            "accessToken": "test-token"
          }
        });
        assert_eq!(
            extract_cli_oauth_access_token(&json),
            Some("test-token".to_string())
        );
    }

    #[test]
    fn missing_access_token_is_none() {
        let json: Value = serde_json::json!({ "claudeAiOauth": {} });
        assert_eq!(extract_cli_oauth_access_token(&json), None);
    }

    #[test]
    fn read_access_token_from_path_reports_missing_file() {
        let path = std::env::temp_dir().join(format!(
            "claudometer-missing-credentials-{}-{}.json",
            std::process::id(),
            time::OffsetDateTime::now_utc().unix_timestamp_nanos()
        ));
        let err = read_cli_oauth_access_token_from_path(&path).unwrap_err();
        assert!(matches!(err, CliCredentialsError::MissingFile));
    }

    #[test]
    fn read_access_token_from_path_reports_invalid_json() {
        let path = std::env::temp_dir().join(format!(
            "claudometer-invalid-credentials-{}-{}.json",
            std::process::id(),
            time::OffsetDateTime::now_utc().unix_timestamp_nanos()
        ));
        let _ = std::fs::write(&path, "{not json");
        let err = read_cli_oauth_access_token_from_path(&path).unwrap_err();
        let _ = std::fs::remove_file(&path);
        assert!(matches!(err, CliCredentialsError::InvalidJson));
    }

    #[test]
    fn parse_oauth_usage_ok_fixture_includes_sonnet_and_opus() {
        let json: Value = serde_json::from_str(include_str!("fixtures/oauth_usage_ok.json"))
            .expect("fixture json");
        let snapshot = parse_usage_from_json(json, "oauth", "2026-01-01T00:00:00.000Z");
        let ClaudeUsageSnapshot::Ok { models, .. } = snapshot else {
            panic!("expected ok snapshot");
        };
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].name, "Sonnet");
        assert_eq!(models[0].percent, 20.0);
        assert_eq!(models[1].name, "Opus");
        assert_eq!(models[1].percent, 9.0);
    }

    #[test]
    fn parse_oauth_usage_null_models_fixture_skips_null_bucket() {
        let json: Value =
            serde_json::from_str(include_str!("fixtures/oauth_usage_null_models.json"))
                .expect("fixture json");
        let snapshot = parse_usage_from_json(json, "oauth", "2026-01-01T00:00:00.000Z");
        let ClaudeUsageSnapshot::Ok { models, .. } = snapshot else {
            panic!("expected ok snapshot");
        };
        assert!(models.iter().all(|m| m.name != "Sonnet"));
        assert!(models.iter().any(|m| m.name == "Opus"));
        assert!(models.iter().any(|m| m.name == "Foo"));
    }

    #[test]
    fn map_http_status_for_oauth() {
        assert!(matches!(
            map_http_status(401),
            ClaudeWebErrorStatus::Unauthorized
        ));
        assert!(matches!(
            map_http_status(403),
            ClaudeWebErrorStatus::Unauthorized
        ));
        assert!(matches!(
            map_http_status(429),
            ClaudeWebErrorStatus::RateLimited
        ));
        assert!(matches!(map_http_status(500), ClaudeWebErrorStatus::Error));
    }
}
