use crate::redact::redact_secrets;
use crate::types::CodexUsageSnapshot;
use reqwest::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT, AUTHORIZATION, COOKIE, ORIGIN, REFERER, USER_AGENT,
};
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;
use std::process::Stdio;
use thiserror::Error;
use time::OffsetDateTime;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

const USAGE_URL_PRIMARY: &str = "https://chatgpt.com/backend-api/wham/usage";
const USAGE_URL_FALLBACK: &str = "https://chatgpt.com/api/codex/usage";

const CODEX_AUTH_RELATIVE_PATH: &str = ".codex/auth.json";

fn now_iso() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn clamp_percent(value: f64) -> f64 {
    if value.is_nan() {
        return 0.0;
    }
    value.clamp(0.0, 100.0)
}

fn epoch_seconds_to_rfc3339(seconds: i64) -> Option<String> {
    let dt = OffsetDateTime::from_unix_timestamp(seconds).ok()?;
    dt.format(&time::format_description::well_known::Rfc3339)
        .ok()
}

fn build_common_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
        ),
    );
    headers.insert(ORIGIN, HeaderValue::from_static("https://chatgpt.com"));
    headers.insert(REFERER, HeaderValue::from_static("https://chatgpt.com/codex/settings/usage"));
    headers
}

fn build_oauth_headers(access_token: &str, account_id: Option<&str>) -> HeaderMap {
    let mut headers = build_common_headers();
    if let Ok(value) = HeaderValue::from_str(&format!("Bearer {access_token}")) {
        headers.insert(AUTHORIZATION, value);
    }
    if let Some(account_id) = account_id.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Ok(value) = HeaderValue::from_str(account_id) {
            headers.insert(HeaderName::from_static("chatgpt-account-id"), value);
        }
    }
    headers
}

fn normalize_cookie_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("cookie:") {
        let original_rest = &trimmed[trimmed.len() - rest.len()..];
        let out = original_rest.trim();
        return (!out.is_empty()).then(|| out.to_string());
    }
    Some(trimmed.to_string())
}

fn build_cookie_headers(cookie_value: &str) -> HeaderMap {
    let mut headers = build_common_headers();
    if let Ok(value) = HeaderValue::from_str(cookie_value) {
        headers.insert(COOKIE, value);
    }
    headers
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexHttpErrorStatus {
    Unauthorized,
    RateLimited,
    Error,
}

#[derive(Debug, Error)]
pub enum CodexError {
    #[error("network error")]
    Network(#[from] reqwest::Error),
    #[error("invalid json")]
    Json(#[from] serde_json::Error),
}

fn map_http_status(status_code: u16) -> CodexHttpErrorStatus {
    match status_code {
        401 | 403 => CodexHttpErrorStatus::Unauthorized,
        429 => CodexHttpErrorStatus::RateLimited,
        _ => CodexHttpErrorStatus::Error,
    }
}

#[derive(Debug, Error, Clone, Copy)]
pub enum CodexCredentialsError {
    #[error("HOME is not set")]
    HomeMissing,
    #[error("credentials file not found")]
    MissingFile,
    #[error("missing access token")]
    MissingAccessToken,
    #[error("invalid json")]
    InvalidJson,
}

#[derive(Debug, Clone)]
pub struct CodexOAuthCredentials {
    pub access_token: String,
    pub account_id: Option<String>,
}

fn auth_file_path() -> Result<PathBuf, CodexCredentialsError> {
    if let Ok(codex_home) = std::env::var("CODEX_HOME") {
        let trimmed = codex_home.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed).join("auth.json"));
        }
    };

    let home = std::env::var("HOME").map_err(|_| CodexCredentialsError::HomeMissing)?;
    Ok(PathBuf::from(home).join(CODEX_AUTH_RELATIVE_PATH))
}

pub fn read_codex_oauth_credentials() -> Result<CodexOAuthCredentials, CodexCredentialsError> {
    let path = auth_file_path()?;
    if !path.exists() {
        return Err(CodexCredentialsError::MissingFile);
    }
    let data = std::fs::read_to_string(&path).map_err(|_| CodexCredentialsError::MissingFile)?;
    let json: Value = serde_json::from_str(&data).map_err(|_| CodexCredentialsError::InvalidJson)?;

    let tokens = json.get("tokens").and_then(|v| v.as_object());
    let access_token = tokens
        .and_then(|t| t.get("access_token"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or(CodexCredentialsError::MissingAccessToken)?;

    let account_id = tokens
        .and_then(|t| t.get("account_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    Ok(CodexOAuthCredentials {
        access_token,
        account_id,
    })
}

#[derive(Debug, Deserialize)]
struct CodexUsageResponse {
    #[serde(default)]
    rate_limit: Option<CodexRateLimit>,
}

#[derive(Debug, Deserialize)]
struct CodexRateLimit {
    #[serde(default)]
    primary_window: Option<CodexWindow>,
    #[serde(default)]
    secondary_window: Option<CodexWindow>,
}

#[derive(Debug, Deserialize)]
struct CodexWindow {
    used_percent: i64,
    reset_at: i64,
}

fn parse_codex_usage_response(json: Value) -> Option<(CodexWindow, CodexWindow)> {
    let parsed: CodexUsageResponse = serde_json::from_value(json).ok()?;
    let rate = parsed.rate_limit?;
    let primary = rate.primary_window?;
    let secondary = rate.secondary_window?;
    Some((primary, secondary))
}

fn ok_snapshot(primary: CodexWindow, secondary: CodexWindow) -> CodexUsageSnapshot {
    CodexUsageSnapshot::Ok {
        session_percent: clamp_percent(primary.used_percent as f64),
        session_resets_at: epoch_seconds_to_rfc3339(primary.reset_at),
        weekly_percent: clamp_percent(secondary.used_percent as f64),
        weekly_resets_at: epoch_seconds_to_rfc3339(secondary.reset_at),
        last_updated_at: now_iso(),
    }
}

fn unauthorized_snapshot(message: &str) -> CodexUsageSnapshot {
    CodexUsageSnapshot::Unauthorized {
        last_updated_at: now_iso(),
        error_message: Some(message.to_string()),
    }
}

fn rate_limited_snapshot(message: &str) -> CodexUsageSnapshot {
    CodexUsageSnapshot::RateLimited {
        last_updated_at: now_iso(),
        error_message: Some(message.to_string()),
    }
}

fn error_snapshot(message: &str) -> CodexUsageSnapshot {
    CodexUsageSnapshot::Error {
        last_updated_at: now_iso(),
        error_message: Some(message.to_string()),
    }
}

fn missing_key_snapshot(message: &str) -> CodexUsageSnapshot {
    CodexUsageSnapshot::MissingKey {
        last_updated_at: now_iso(),
        error_message: Some(message.to_string()),
    }
}

pub struct CodexApiClient {
    http: reqwest::Client,
}

impl CodexApiClient {
    pub fn new() -> Result<Self, CodexError> {
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(40))
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()?,
        })
    }

    async fn fetch_usage_json(&self, headers: HeaderMap) -> Result<Value, CodexHttpErrorStatus> {
        async fn attempt(
            http: &reqwest::Client,
            url: &'static str,
            headers: &HeaderMap,
        ) -> Result<Value, CodexHttpErrorStatus> {
            let res = http.get(url).headers(headers.clone()).send().await;
            let res = match res {
                Ok(r) => r,
                Err(_) => return Err(CodexHttpErrorStatus::Error),
            };

            if !res.status().is_success() {
                return Err(map_http_status(res.status().as_u16()));
            }

            let json: Value = match res.json().await {
                Ok(v) => v,
                Err(_) => return Err(CodexHttpErrorStatus::Error),
            };
            Ok(json)
        }

        match attempt(&self.http, USAGE_URL_PRIMARY, &headers).await {
            Ok(v) => Ok(v),
            Err(CodexHttpErrorStatus::Error) => attempt(&self.http, USAGE_URL_FALLBACK, &headers).await,
            Err(e) => Err(e),
        }
    }

    pub async fn fetch_oauth_usage_snapshot(
        &self,
        access_token: &str,
        account_id: Option<&str>,
    ) -> CodexUsageSnapshot {
        let headers = build_oauth_headers(access_token, account_id);
        let result = self.fetch_usage_json(headers).await;
        match result {
            Ok(json) => match parse_codex_usage_response(json) {
                Some((primary, secondary)) => ok_snapshot(primary, secondary),
                None => error_snapshot("Codex usage data missing required fields."),
            },
            Err(CodexHttpErrorStatus::Unauthorized) => {
                unauthorized_snapshot("Codex OAuth credentials are invalid. Run `codex` to re-authenticate.")
            }
            Err(CodexHttpErrorStatus::RateLimited) => rate_limited_snapshot("Rate limited."),
            Err(CodexHttpErrorStatus::Error) => error_snapshot("Failed to fetch Codex usage."),
        }
    }

    pub async fn fetch_web_cookie_usage_snapshot(&self, cookie_value: &str) -> CodexUsageSnapshot {
        let Some(cookie_value) = normalize_cookie_value(cookie_value) else {
            return missing_key_snapshot("Codex cookie is required.");
        };
        let headers = build_cookie_headers(&cookie_value);
        let result = self.fetch_usage_json(headers).await;
        match result {
            Ok(json) => match parse_codex_usage_response(json) {
                Some((primary, secondary)) => ok_snapshot(primary, secondary),
                None => error_snapshot("Codex usage data missing required fields."),
            },
            Err(CodexHttpErrorStatus::Unauthorized) => {
                unauthorized_snapshot("Codex cookie is invalid or expired.")
            }
            Err(CodexHttpErrorStatus::RateLimited) => rate_limited_snapshot("Rate limited."),
            Err(CodexHttpErrorStatus::Error) => error_snapshot("Failed to fetch Codex usage."),
        }
    }

    pub async fn fetch_cli_usage_snapshot(&self, codex_binary: &str) -> CodexUsageSnapshot {
        match CodexRpcClient::fetch_rate_limits(codex_binary).await {
            Ok((primary, secondary)) => ok_snapshot(primary, secondary),
            Err(CodexCliError::BinaryMissing) => {
                error_snapshot("Codex CLI missing. Install `@openai/codex` (or ensure `codex` is on PATH).")
            }
            Err(CodexCliError::TimedOut) => error_snapshot("Codex CLI probe timed out."),
            Err(CodexCliError::Malformed) => error_snapshot("Codex CLI returned invalid data."),
            Err(CodexCliError::Failed(msg)) => error_snapshot(&msg),
        }
    }
}

#[derive(Debug)]
enum CodexCliError {
    BinaryMissing,
    TimedOut,
    Malformed,
    Failed(String),
}

#[derive(Debug, Deserialize)]
struct RpcRateLimitsResponse {
    #[serde(rename = "rateLimits")]
    rate_limits: RpcRateLimitSnapshot,
}

#[derive(Debug, Deserialize)]
struct RpcRateLimitSnapshot {
    primary: Option<RpcRateLimitWindow>,
    secondary: Option<RpcRateLimitWindow>,
}

#[derive(Debug, Deserialize)]
struct RpcRateLimitWindow {
    #[serde(rename = "usedPercent")]
    used_percent: f64,
    #[serde(rename = "resetsAt")]
    resets_at: Option<i64>,
}

impl RpcRateLimitWindow {
    fn to_codex_window(&self) -> Option<CodexWindow> {
        let reset_at = self.resets_at?;
        Some(CodexWindow {
            used_percent: self.used_percent.round() as i64,
            reset_at,
        })
    }
}

struct CodexRpcClient;

impl CodexRpcClient {
    async fn fetch_rate_limits(binary: &str) -> Result<(CodexWindow, CodexWindow), CodexCliError> {
        let mut child = Command::new(binary)
            .args(["-s", "read-only", "-a", "untrusted", "app-server"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| CodexCliError::BinaryMissing)?;

        let mut stdin = child.stdin.take().ok_or(CodexCliError::Malformed)?;
        let stdout = child.stdout.take().ok_or(CodexCliError::Malformed)?;
        let stderr = child.stderr.take().ok_or(CodexCliError::Malformed)?;

        let mut lines = BufReader::new(stdout).lines();
        let mut stderr_lines = BufReader::new(stderr).lines();

        let init_id = 1_i64;
        Self::send_request(
            &mut stdin,
            init_id,
            "initialize",
            serde_json::json!({"clientInfo": {"name": "claudometer", "version": env!("CARGO_PKG_VERSION")}}),
        )
        .await?;
        let _ = Self::read_response(&mut lines, &mut stderr_lines, init_id).await?;

        Self::send_notification(&mut stdin, "initialized", serde_json::json!({}))
            .await
            .ok();

        let limits_id = 2_i64;
        Self::send_request(&mut stdin, limits_id, "account/rateLimits/read", serde_json::json!({}))
            .await?;
        let message = Self::read_response(&mut lines, &mut stderr_lines, limits_id).await?;

        let result = message
            .get("result")
            .cloned()
            .ok_or(CodexCliError::Malformed)?;
        let parsed: RpcRateLimitsResponse =
            serde_json::from_value(result).map_err(|_| CodexCliError::Malformed)?;

        let primary = parsed
            .rate_limits
            .primary
            .as_ref()
            .and_then(RpcRateLimitWindow::to_codex_window)
            .ok_or(CodexCliError::Malformed)?;
        let secondary = parsed
            .rate_limits
            .secondary
            .as_ref()
            .and_then(RpcRateLimitWindow::to_codex_window)
            .ok_or(CodexCliError::Malformed)?;

        let _ = child.kill().await;
        Ok((primary, secondary))
    }

    async fn send_request(
        stdin: &mut tokio::process::ChildStdin,
        id: i64,
        method: &str,
        params: Value,
    ) -> Result<(), CodexCliError> {
        let payload = serde_json::json!({"id": id, "method": method, "params": params});
        Self::write_line(stdin, payload).await
    }

    async fn send_notification(
        stdin: &mut tokio::process::ChildStdin,
        method: &str,
        params: Value,
    ) -> Result<(), CodexCliError> {
        let payload = serde_json::json!({"method": method, "params": params});
        Self::write_line(stdin, payload).await
    }

    async fn write_line(stdin: &mut tokio::process::ChildStdin, payload: Value) -> Result<(), CodexCliError> {
        let data = serde_json::to_vec(&payload).map_err(|_| CodexCliError::Malformed)?;
        stdin.write_all(&data).await.map_err(|_| CodexCliError::Failed("Codex CLI write failed.".to_string()))?;
        stdin.write_all(b"\n").await.map_err(|_| CodexCliError::Failed("Codex CLI write failed.".to_string()))?;
        Ok(())
    }

    async fn read_response(
        lines: &mut tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
        stderr_lines: &mut tokio::io::Lines<BufReader<tokio::process::ChildStderr>>,
        id: i64,
    ) -> Result<Value, CodexCliError> {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(12);
        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Err(CodexCliError::TimedOut);
            }
            let timeout = deadline - now;

            tokio::select! {
                biased;
                line = tokio::time::timeout(timeout, lines.next_line()) => {
                    let line = line.map_err(|_| CodexCliError::TimedOut)?.map_err(|_| CodexCliError::Malformed)?;
                    let Some(line) = line else {
                        return Err(CodexCliError::Malformed);
                    };
                    let json: Value = serde_json::from_str(&line).map_err(|_| CodexCliError::Malformed)?;
                    if json.get("id").and_then(|v| v.as_i64()) != Some(id) {
                        continue;
                    }
                    if let Some(err) = json.get("error").and_then(|v| v.get("message")).and_then(|v| v.as_str()) {
                        return Err(CodexCliError::Failed(redact_secrets(err).to_string()));
                    }
                    return Ok(json);
                }
                line = tokio::time::timeout(timeout, stderr_lines.next_line()) => {
                    let line = line.map_err(|_| CodexCliError::TimedOut)?.map_err(|_| CodexCliError::Malformed)?;
                    if let Some(line) = line {
                        let _ = redact_secrets(&line);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_cookie_value_strips_cookie_prefix() {
        assert_eq!(
            normalize_cookie_value("Cookie: a=b; c=d").as_deref(),
            Some("a=b; c=d")
        );
        assert_eq!(normalize_cookie_value("a=b; c=d").as_deref(), Some("a=b; c=d"));
        assert_eq!(normalize_cookie_value("  ").as_deref(), None);
    }

    #[test]
    fn parse_oauth_fixture_maps_windows() {
        let data = include_str!("fixtures/codex_oauth_usage_ok.json");
        let json: Value = serde_json::from_str(data).unwrap();
        let (primary, secondary) = parse_codex_usage_response(json).unwrap();
        assert_eq!(primary.used_percent, 25);
        assert_eq!(secondary.used_percent, 40);
    }

    #[test]
    fn parse_rpc_fixture_maps_windows() {
        let data = include_str!("fixtures/codex_rpc_rate_limits_ok.json");
        let message: Value = serde_json::from_str(data).unwrap();
        let result = message.get("result").cloned().unwrap();
        let parsed: RpcRateLimitsResponse = serde_json::from_value(result).unwrap();
        let primary = parsed
            .rate_limits
            .primary
            .as_ref()
            .and_then(RpcRateLimitWindow::to_codex_window)
            .unwrap();
        assert_eq!(primary.used_percent, 33);
    }
}
