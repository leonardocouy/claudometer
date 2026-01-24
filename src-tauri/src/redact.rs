use std::borrow::Cow;

pub fn redact_session_key(input: &str) -> Cow<'_, str> {
    let mut redacted = input.to_string();

    for key in ["sessionKey=", "sessionkey="] {
        if !redacted.contains(key) {
            continue;
        }
        let mut out = String::with_capacity(redacted.len());
        let mut rest = redacted.as_str();
        while let Some(idx) = rest.find(key) {
            out.push_str(&rest[..idx]);
            out.push_str(&rest[idx..idx + key.len()]);
            rest = &rest[idx + key.len()..];

            let mut consumed = 0;
            for ch in rest.chars() {
                if ch == ';' || ch.is_whitespace() {
                    break;
                }
                consumed += ch.len_utf8();
            }
            out.push_str("REDACTED");
            rest = &rest[consumed..];
        }
        out.push_str(rest);
        redacted = out;
    }

    // Also redact the common Claude cookie token prefix.
    if redacted.contains("sk-ant-sid01-") {
        let mut out = String::with_capacity(redacted.len());
        let mut rest = redacted.as_str();
        while let Some(idx) = rest.find("sk-ant-sid01-") {
            out.push_str(&rest[..idx]);
            out.push_str("sk-ant-sid01-REDACTED");
            rest = &rest[idx + "sk-ant-sid01-".len()..];
            // Consume the token tail.
            let mut consumed = 0;
            for ch in rest.chars() {
                if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                    consumed += ch.len_utf8();
                } else {
                    break;
                }
            }
            rest = &rest[consumed..];
        }
        out.push_str(rest);
        redacted = out;
    }

    if redacted == input {
        Cow::Borrowed(input)
    } else {
        Cow::Owned(redacted)
    }
}

fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    let hay = haystack.as_bytes();
    let nee = needle.as_bytes();
    if nee.is_empty() {
        return Some(0);
    }
    if nee.len() > hay.len() {
        return None;
    }

    for i in 0..=hay.len() - nee.len() {
        let mut matches = true;
        for j in 0..nee.len() {
            let a = hay[i + j].to_ascii_lowercase();
            let b = nee[j].to_ascii_lowercase();
            if a != b {
                matches = false;
                break;
            }
        }
        if matches {
            return Some(i);
        }
    }
    None
}

fn redact_header_value(text: String, header: &str, replacement: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    loop {
        let Some(idx) = find_ascii_case_insensitive(rest, header) else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..idx]);
        rest = &rest[idx..];

        // Consume the header name itself as-is from original input.
        if rest.len() < header.len() {
            out.push_str(rest);
            break;
        }
        out.push_str(&rest[..header.len()]);
        rest = &rest[header.len()..];

        // If there is a separating space, preserve it.
        if let Some(first) = rest.chars().next() {
            if first == ' ' {
                out.push(' ');
                rest = &rest[first.len_utf8()..];
            }
        }

        // Consume until end-of-line.
        let mut consumed = 0;
        for ch in rest.chars() {
            if ch == '\n' || ch == '\r' {
                break;
            }
            consumed += ch.len_utf8();
        }
        out.push_str(replacement);
        rest = &rest[consumed..];
    }
    out
}

pub fn redact_secrets(input: &str) -> Cow<'_, str> {
    let after_session = redact_session_key(input);
    let mut value = match after_session {
        Cow::Borrowed(s) => s.to_string(),
        Cow::Owned(s) => s,
    };

    value = redact_header_value(value, "Cookie:", "REDACTED");
    value = redact_header_value(value, "cookie:", "REDACTED");
    value = redact_header_value(value, "Authorization: Bearer", "REDACTED");
    value = redact_header_value(value, "authorization: Bearer", "REDACTED");

    if value == input {
        Cow::Borrowed(input)
    } else {
        Cow::Owned(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_secrets_redacts_cookie_header_line() {
        let input = "Cookie: sessionKey=abc123;\nOther: ok\n";
        let out = redact_secrets(input).to_string();
        assert!(out.contains("Cookie: REDACTED\n"));
        assert!(out.contains("Other: ok\n"));
        assert!(!out.contains("abc123"));
    }

    #[test]
    fn redact_secrets_redacts_bearer_header_line() {
        let input = "Authorization: Bearer sk-live-xyz\n";
        let out = redact_secrets(input).to_string();
        assert_eq!(out, "Authorization: Bearer REDACTED\n");
        assert!(!out.contains("sk-live-xyz"));
    }
}
