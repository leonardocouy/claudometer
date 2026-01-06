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
