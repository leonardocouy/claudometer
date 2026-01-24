use crate::types::{ClaudeUsageSnapshot, CodexUsageSnapshot};

pub struct ProviderOkView<'a> {
    pub provider_label: &'static str,
    pub scope_id: &'a str,
    pub session_percent: f64,
    pub weekly_percent: f64,
    pub session_resets_at: Option<&'a str>,
    pub weekly_resets_at: Option<&'a str>,
}

pub fn view_claude(snapshot: &ClaudeUsageSnapshot) -> Option<ProviderOkView<'_>> {
    match snapshot {
        ClaudeUsageSnapshot::Ok {
            organization_id,
            session_percent,
            session_resets_at,
            weekly_percent,
            weekly_resets_at,
            last_updated_at: _,
            ..
        } => Some(ProviderOkView {
            provider_label: "Claude",
            scope_id: organization_id,
            session_percent: *session_percent,
            weekly_percent: *weekly_percent,
            session_resets_at: session_resets_at.as_deref(),
            weekly_resets_at: weekly_resets_at.as_deref(),
        }),
        _ => None,
    }
}

pub fn view_codex(snapshot: &CodexUsageSnapshot) -> Option<ProviderOkView<'_>> {
    match snapshot {
        CodexUsageSnapshot::Ok {
            session_percent,
            session_resets_at,
            weekly_percent,
            weekly_resets_at,
            last_updated_at: _,
        } => Some(ProviderOkView {
            provider_label: "Codex",
            scope_id: "codex",
            session_percent: *session_percent,
            weekly_percent: *weekly_percent,
            session_resets_at: session_resets_at.as_deref(),
            weekly_resets_at: weekly_resets_at.as_deref(),
        }),
        _ => None,
    }
}
