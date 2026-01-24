use crate::claude::{read_cli_oauth_access_token, ClaudeWebErrorStatus, CliCredentialsError};
use crate::codex::read_codex_oauth_credentials;
use crate::state::AppState;
use crate::types::{
    ClaudeUsageSnapshot, CodexUsageSnapshot, CodexUsageSource, UsageSnapshotBundle, UsageSource,
};
use tauri::Runtime;

fn now_iso() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

pub(crate) fn bundle(
    claude: Option<ClaudeUsageSnapshot>,
    codex: Option<CodexUsageSnapshot>,
) -> UsageSnapshotBundle {
    UsageSnapshotBundle { claude, codex }
}

pub(crate) fn claude_missing_key_snapshot() -> ClaudeUsageSnapshot {
    ClaudeUsageSnapshot::MissingKey {
        organization_id: None,
        last_updated_at: now_iso(),
        error_message: Some("Session key is not configured.".to_string()),
    }
}

fn claude_unauthorized_snapshot(message: &str) -> ClaudeUsageSnapshot {
    ClaudeUsageSnapshot::Unauthorized {
        organization_id: None,
        last_updated_at: now_iso(),
        error_message: Some(message.to_string()),
    }
}

fn claude_rate_limited_snapshot(message: &str) -> ClaudeUsageSnapshot {
    ClaudeUsageSnapshot::RateLimited {
        organization_id: None,
        last_updated_at: now_iso(),
        error_message: Some(message.to_string()),
    }
}

fn claude_error_snapshot(message: &str) -> ClaudeUsageSnapshot {
    ClaudeUsageSnapshot::Error {
        organization_id: None,
        last_updated_at: now_iso(),
        error_message: Some(message.to_string()),
    }
}

async fn resolve_organization_id<R: Runtime>(
    state: &AppState<R>,
    session_key: &str,
) -> Result<Option<String>, ClaudeWebErrorStatus> {
    let orgs = state.get_organizations_cached(session_key).await?;

    {
        let mut guard = state.organizations.lock().await;
        *guard = orgs.clone();
    }

    let stored = state.selected_org_id();
    if let Some(stored) = stored {
        if orgs.iter().any(|o| o.id == stored) {
            return Ok(Some(stored));
        }
    }

    let first = orgs.first().map(|o| o.id.clone());
    if let Some(id) = &first {
        state
            .settings
            .set(crate::settings::KEY_SELECTED_ORGANIZATION_ID, id.clone());
    }
    Ok(first)
}

pub(crate) struct FetchSnapshot<T> {
    pub(crate) snapshot: T,
    pub(crate) keyring_error: bool,
}

pub(crate) async fn fetch_claude_snapshot<R: Runtime>(
    state: &AppState<R>,
) -> FetchSnapshot<ClaudeUsageSnapshot> {
    match state.usage_source() {
        UsageSource::Web => {
            let remember = state.remember_session_key();
            let session_key = match state.claude_session_key.get_current(remember).await {
                Ok(Some(k)) => k,
                Ok(None) => {
                    return FetchSnapshot {
                        snapshot: claude_missing_key_snapshot(),
                        keyring_error: false,
                    };
                }
                Err(()) => {
                    return FetchSnapshot {
                        snapshot: ClaudeUsageSnapshot::MissingKey {
                            organization_id: None,
                            last_updated_at: now_iso(),
                            error_message: Some(
                                "OS keychain/secret service is unavailable.".to_string(),
                            ),
                        },
                        keyring_error: true,
                    };
                }
            };

            let org_id = match resolve_organization_id(state, &session_key).await {
                Ok(Some(id)) => id,
                Ok(None) => {
                    return FetchSnapshot {
                        snapshot: claude_error_snapshot("No organizations found."),
                        keyring_error: false,
                    };
                }
                Err(ClaudeWebErrorStatus::Unauthorized) => {
                    return FetchSnapshot {
                        snapshot: claude_unauthorized_snapshot("Unauthorized."),
                        keyring_error: false,
                    };
                }
                Err(ClaudeWebErrorStatus::RateLimited) => {
                    return FetchSnapshot {
                        snapshot: claude_rate_limited_snapshot("Rate limited."),
                        keyring_error: false,
                    };
                }
                Err(ClaudeWebErrorStatus::Error) => {
                    return FetchSnapshot {
                        snapshot: claude_error_snapshot("Failed to fetch organizations."),
                        keyring_error: false,
                    };
                }
            };

            FetchSnapshot {
                snapshot: state
                    .claude
                    .fetch_usage_snapshot(&session_key, &org_id)
                    .await,
                keyring_error: false,
            }
        }
        UsageSource::Cli => {
            let access_token = match read_cli_oauth_access_token() {
                Ok(t) => t,
                Err(CliCredentialsError::HomeMissing) => {
                    return FetchSnapshot {
                        snapshot: claude_error_snapshot(
                            "HOME is not set; cannot locate CLI credentials.",
                        ),
                        keyring_error: false,
                    };
                }
                Err(CliCredentialsError::MissingFile | CliCredentialsError::MissingAccessToken) => {
                    return FetchSnapshot {
                        snapshot: claude_unauthorized_snapshot(
                            "Claude CLI credentials not found. Run `claude login` and try again.",
                        ),
                        keyring_error: false,
                    };
                }
                Err(CliCredentialsError::InvalidJson) => {
                    return FetchSnapshot {
                        snapshot: claude_unauthorized_snapshot(
                            "Claude CLI credentials are invalid. Re-authenticate (run `claude login`).",
                        ),
                        keyring_error: false,
                    };
                }
            };

            FetchSnapshot {
                snapshot: state.claude.fetch_oauth_usage_snapshot(&access_token).await,
                keyring_error: false,
            }
        }
    }
}

pub(crate) async fn fetch_codex_snapshot<R: Runtime>(
    state: &AppState<R>,
) -> FetchSnapshot<CodexUsageSnapshot> {
    match state.codex_usage_source() {
        CodexUsageSource::Oauth => match read_codex_oauth_credentials() {
            Ok(creds) => FetchSnapshot {
                snapshot: state
                    .codex
                    .fetch_oauth_usage_snapshot(&creds.access_token, creds.account_id.as_deref())
                    .await,
                keyring_error: false,
            },
            Err(_) => FetchSnapshot {
                snapshot: CodexUsageSnapshot::Unauthorized {
                    last_updated_at: now_iso(),
                    error_message: Some(
                        "Codex credentials not found. Run `codex` to log in.".to_string(),
                    ),
                },
                keyring_error: false,
            },
        },
        CodexUsageSource::Cli => FetchSnapshot {
            snapshot: state.codex.fetch_cli_usage_snapshot("codex").await,
            keyring_error: false,
        },
    }
}
