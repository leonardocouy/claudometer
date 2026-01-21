use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageStatus {
    Ok,
    Unauthorized,
    RateLimited,
    Error,
    MissingKey,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageProvider {
    Claude,
    Codex,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageSource {
    Web,
    Cli,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodexUsageSource {
    Auto,
    Oauth,
    Web,
    Cli,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeModelUsage {
    pub name: String,
    pub percent: f64,
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ClaudeUsageSnapshot {
    Ok {
        #[serde(rename = "organizationId")]
        organization_id: String,
        #[serde(rename = "sessionPercent")]
        session_percent: f64,
        #[serde(rename = "sessionResetsAt")]
        session_resets_at: Option<String>,
        #[serde(rename = "weeklyPercent")]
        weekly_percent: f64,
        #[serde(rename = "weeklyResetsAt")]
        weekly_resets_at: Option<String>,
        #[serde(rename = "models")]
        models: Vec<ClaudeModelUsage>,
        #[serde(rename = "lastUpdatedAt")]
        last_updated_at: String,
    },
    Unauthorized {
        #[serde(rename = "organizationId")]
        organization_id: Option<String>,
        #[serde(rename = "lastUpdatedAt")]
        last_updated_at: String,
        #[serde(rename = "errorMessage")]
        error_message: Option<String>,
    },
    RateLimited {
        #[serde(rename = "organizationId")]
        organization_id: Option<String>,
        #[serde(rename = "lastUpdatedAt")]
        last_updated_at: String,
        #[serde(rename = "errorMessage")]
        error_message: Option<String>,
    },
    Error {
        #[serde(rename = "organizationId")]
        organization_id: Option<String>,
        #[serde(rename = "lastUpdatedAt")]
        last_updated_at: String,
        #[serde(rename = "errorMessage")]
        error_message: Option<String>,
    },
    MissingKey {
        #[serde(rename = "organizationId")]
        organization_id: Option<String>,
        #[serde(rename = "lastUpdatedAt")]
        last_updated_at: String,
        #[serde(rename = "errorMessage")]
        error_message: Option<String>,
    },
}

impl ClaudeUsageSnapshot {
    pub fn status(&self) -> UsageStatus {
        match self {
            Self::Ok { .. } => UsageStatus::Ok,
            Self::Unauthorized { .. } => UsageStatus::Unauthorized,
            Self::RateLimited { .. } => UsageStatus::RateLimited,
            Self::Error { .. } => UsageStatus::Error,
            Self::MissingKey { .. } => UsageStatus::MissingKey,
        }
    }

    pub fn last_updated_at(&self) -> &str {
        match self {
            Self::Ok {
                last_updated_at, ..
            } => last_updated_at,
            Self::Unauthorized {
                last_updated_at, ..
            } => last_updated_at,
            Self::RateLimited {
                last_updated_at, ..
            } => last_updated_at,
            Self::Error {
                last_updated_at, ..
            } => last_updated_at,
            Self::MissingKey {
                last_updated_at, ..
            } => last_updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CodexUsageSnapshot {
    Ok {
        #[serde(rename = "sessionPercent")]
        session_percent: f64,
        #[serde(rename = "sessionResetsAt")]
        session_resets_at: Option<String>,
        #[serde(rename = "weeklyPercent")]
        weekly_percent: f64,
        #[serde(rename = "weeklyResetsAt")]
        weekly_resets_at: Option<String>,
        #[serde(rename = "lastUpdatedAt")]
        last_updated_at: String,
    },
    Unauthorized {
        #[serde(rename = "lastUpdatedAt")]
        last_updated_at: String,
        #[serde(rename = "errorMessage")]
        error_message: Option<String>,
    },
    RateLimited {
        #[serde(rename = "lastUpdatedAt")]
        last_updated_at: String,
        #[serde(rename = "errorMessage")]
        error_message: Option<String>,
    },
    Error {
        #[serde(rename = "lastUpdatedAt")]
        last_updated_at: String,
        #[serde(rename = "errorMessage")]
        error_message: Option<String>,
    },
    MissingKey {
        #[serde(rename = "lastUpdatedAt")]
        last_updated_at: String,
        #[serde(rename = "errorMessage")]
        error_message: Option<String>,
    },
}

impl CodexUsageSnapshot {
    pub fn status(&self) -> UsageStatus {
        match self {
            Self::Ok { .. } => UsageStatus::Ok,
            Self::Unauthorized { .. } => UsageStatus::Unauthorized,
            Self::RateLimited { .. } => UsageStatus::RateLimited,
            Self::Error { .. } => UsageStatus::Error,
            Self::MissingKey { .. } => UsageStatus::MissingKey,
        }
    }

    pub fn last_updated_at(&self) -> &str {
        match self {
            Self::Ok {
                last_updated_at, ..
            } => last_updated_at,
            Self::Unauthorized {
                last_updated_at, ..
            } => last_updated_at,
            Self::RateLimited {
                last_updated_at, ..
            } => last_updated_at,
            Self::Error {
                last_updated_at, ..
            } => last_updated_at,
            Self::MissingKey {
                last_updated_at, ..
            } => last_updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum UsageSnapshot {
    Claude {
        #[serde(flatten)]
        snapshot: ClaudeUsageSnapshot,
    },
    Codex {
        #[serde(flatten)]
        snapshot: CodexUsageSnapshot,
    },
}

impl UsageSnapshot {
    pub fn provider(&self) -> UsageProvider {
        match self {
            Self::Claude { .. } => UsageProvider::Claude,
            Self::Codex { .. } => UsageProvider::Codex,
        }
    }

    pub fn status(&self) -> UsageStatus {
        match self {
            Self::Claude { snapshot } => snapshot.status(),
            Self::Codex { snapshot } => snapshot.status(),
        }
    }

    pub fn last_updated_at(&self) -> &str {
        match self {
            Self::Claude { snapshot } => snapshot.last_updated_at(),
            Self::Codex { snapshot } => snapshot.last_updated_at(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeOrganization {
    pub id: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IpcResult<T> {
    Ok { ok: bool, value: T },
    Err { ok: bool, error: IpcError },
}

impl<T> IpcResult<T> {
    pub fn ok(value: T) -> Self {
        Self::Ok { ok: true, value }
    }

    pub fn err(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Err {
            ok: false,
            error: IpcError {
                code: code.into(),
                message: message.into(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsState {
    pub provider: UsageProvider,
    pub usage_source: UsageSource,
    pub remember_session_key: bool,
    pub codex_usage_source: CodexUsageSource,
    pub remember_codex_cookie: bool,
    pub refresh_interval_seconds: u64,
    pub notify_on_usage_reset: bool,
    pub autostart_enabled: bool,
    pub check_updates_on_startup: bool,
    pub organizations: Vec<ClaudeOrganization>,
    pub selected_organization_id: Option<String>,
    pub latest_snapshot: Option<UsageSnapshot>,
    pub keyring_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSettingsPayload {
    pub provider: UsageProvider,
    pub usage_source: UsageSource,
    pub session_key: Option<String>,
    pub remember_session_key: bool,
    pub codex_usage_source: CodexUsageSource,
    pub codex_cookie: Option<String>,
    pub remember_codex_cookie: bool,
    pub refresh_interval_seconds: u64,
    pub notify_on_usage_reset: bool,
    pub autostart_enabled: bool,
    pub check_updates_on_startup: bool,
    pub selected_organization_id: Option<String>,
}
