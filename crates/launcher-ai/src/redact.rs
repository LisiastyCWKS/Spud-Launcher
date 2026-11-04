//! Redaction before anything leaves the machine.
//!
//! Crash logs carry more than a stack trace: absolute paths with the Windows
//! account name, access tokens passed on the command line, session ids, player
//! UUIDs and often an email. None of that is needed to explain a crash, so it is
//! replaced with a stable placeholder before the log is sent to a provider.
//!
//! The mapping is intentionally lossy and irreversible — a placeholder is not a
//! token that can be swapped back.

use serde::Serialize;

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct RedactionReport {
    pub home_paths: usize,
    pub tokens: usize,
    pub uuids: usize,
    pub emails: usize,
}

impl RedactionReport {
    pub fn total(&self) -> usize {
        self.home_paths + self.tokens + self.uuids + self.emails
    }
}

/// Redact a log. Returns the cleaned text and what was replaced.
pub fn redact(input: &str) -> (String, RedactionReport) {
    let mut report = RedactionReport::default();
    let mut text = input.to_string();

    // `--accessToken <value>` / `--session <value>` / `--uuid` on the command
    // line are the highest-value secrets and are removed first, before the
    // generic UUID pass can eat them.
    for flag in ["--accessToken", "--session", "--clientId", "--xuid"] {
        text = replace_flag_value(&text, flag, "<redacted>", &mut report.tokens);
    }

    // JSON-ish credentials: "accessToken":"…", "refresh_token": "…".
    for key in ["accessToken", "access_token", "refreshToken", "refresh_token", "bearer"] {
        text = replace_json_value(&text, key, "<redacted>", &mut report.tokens);
    }
    if let Some(next) = replace_bearer(&text, &mut report.tokens) {
        text = next;
    }

    // Absolute home directories: C:\Users\<name>, /home/<name>, /Users/<name>.
    text = replace_home(&text, &mut report.home_paths);

    // Dashed UUIDs (player ids, session ids). Offline UUIDs are equally private.
    text = replace_uuids(&text, &mut report.uuids);

    text = replace_emails(&text, &mut report.emails);

    (text, report)
}

fn replace_flag_value(text: &str, flag: &str, with: &str, count: &mut usize) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(found) = rest.find(flag) {
        out.push_str(&rest[..found]);
        out.push_str(flag);
        let after = &rest[found + flag.len()..];
        // Keep the separator so the shape of the command line stays readable.
        let mut chars = after.char_indices();
        let mut consumed = 0;
        if let Some((_, c)) = chars.next() {
            if c == '=' {
                out.push('=');
                consumed = 1;
            } else if c == ' ' {
                out.push(' ');
                consumed = 1;
            }
        }
        let remainder = &after[consumed..];
        let end = remainder
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
            .unwrap_or(remainder.len());
        if end > 0 {
            out.push_str(with);
            *count += 1;
        }
        rest = &remainder[end..];
    }
    out.push_str(rest);
    out
}

fn replace_json_value(text: &str, key: &str, with: &str, count: &mut usize) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(found) = rest.find(key) {
        out.push_str(&rest[..found]);
        out.push_str(key);
        let after = &rest[found + key.len()..];

        // Accept `"key":"value"`, `'key' : value` and bare `key: value`. The
        // separator is copied verbatim so the log still reads like the original
        // and nothing shifts on the line.
        let bytes = after.as_bytes();
        let mut cursor = 0;
        let skip = |cursor: &mut usize, quote: bool| {
            while *cursor < bytes.len() && bytes[*cursor].is_ascii_whitespace() {
                *cursor += 1;
            }
            if quote
                && *cursor < bytes.len()
                && (bytes[*cursor] == b'"' || bytes[*cursor] == b'\'')
            {
                *cursor += 1;
            }
            while *cursor < bytes.len() && bytes[*cursor].is_ascii_whitespace() {
                *cursor += 1;
            }
        };
        skip(&mut cursor, true);
        if cursor >= bytes.len() || bytes[cursor] != b':' {
            // A flag such as `--accessToken` merely contains the key; leave it
            // for `replace_flag_value` and keep scanning the rest of the line.
            rest = after;
            continue;
        }
        cursor += 1;
        skip(&mut cursor, true);

        let body = &after[cursor..];
        let end = body
            .find(|c: char| c == '"' || c == '\'' || c == ',' || c == '}' || c.is_whitespace())
            .unwrap_or(body.len());
        out.push_str(&after[..cursor]);
        if end > 0 {
            out.push_str(with);
            *count += 1;
            rest = &body[end..];
        } else {
            rest = body;
        }
    }
    out.push_str(rest);
    out
}

fn replace_bearer(text: &str, count: &mut usize) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut changed = false;
    while let Some(found) = rest.find("Bearer ") {
        out.push_str(&rest[..found]);
        out.push_str("Bearer ");
        let after = &rest[found + 7..];
        let end = after
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
            .unwrap_or(after.len());
        if end > 0 {
            out.push_str("<redacted>");
            *count += 1;
            changed = true;
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    changed.then_some(out)
}

fn replace_home(text: &str, count: &mut usize) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    loop {
        let mut best: Option<(usize, &'static str)> = None;
        for prefix in ["C:\\Users\\", "C:/Users/", "/home/", "/Users/"] {
            if let Some(index) = rest.find(prefix) {
                if best.map(|(at, _)| index < at).unwrap_or(true) {
                    best = Some((index, prefix));
                }
            }
        }
        let Some((index, prefix)) = best else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..index]);
        out.push_str(prefix);
        out.push_str("<user>");
        let after = &rest[index + prefix.len()..];
        // Skip the account name itself; keep the separator so the path tail is
        // still readable ("…\<user>\AppData\…").
        let end = after
            .find(['\\', '/'])
            .unwrap_or(0);
        *count += 1;
        rest = &after[end..];
    }

    out
}

fn replace_uuids(text: &str, count: &mut usize) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < bytes.len() {
        // A UUID is 36 chars: 8-4-4-4-12 hex with dashes at 8, 13, 18, 23.
        if index + 36 <= bytes.len() && looks_like_uuid(&bytes[index..index + 36]) {
            out.push_str("<uuid>");
            *count += 1;
            index += 36;
            continue;
        }
        let ch = text[index..].chars().next().unwrap();
        out.push(ch);
        index += ch.len_utf8();
    }
    out
}

fn looks_like_uuid(slice: &[u8]) -> bool {
    if slice.len() != 36 {
        return false;
    }
    for (position, byte) in slice.iter().enumerate() {
        let is_dash = matches!(position, 8 | 13 | 18 | 23);
        if is_dash {
            if *byte != b'-' {
                return false;
            }
        } else if !byte.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

fn replace_emails(text: &str, count: &mut usize) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'@' {
            // Walk back for the local part, forward for the domain.
            let mut start = index;
            while start > 0
                && (bytes[start - 1].is_ascii_alphanumeric()
                    || matches!(bytes[start - 1], b'.' | b'_' | b'-' | b'+'))
            {
                start -= 1;
            }
            let mut end = index + 1;
            while end < bytes.len()
                && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'.' | b'-'))
            {
                end += 1;
            }
            let local_len = index - start;
            let domain_len = end - index - 1;
            if local_len >= 1 && domain_len >= 3 && bytes[index + 1..end].contains(&b'.') {
                out.truncate(out.len() - local_len);
                out.push_str("<email>");
                *count += 1;
                index = end;
                continue;
            }
        }
        let ch = text[index..].chars().next().unwrap();
        out.push(ch);
        index += ch.len_utf8();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_home_directory_is_replaced() {
        let (text, report) = redact("C:\\Users\\olivia\\AppData\\Roaming\\.minecraft\\mods\\a.jar");
        assert!(text.contains("C:\\Users\\<user>"));
        assert!(!text.contains("olivia"));
        assert_eq!(report.home_paths, 1);
    }

    #[test]
    fn unix_homes_are_replaced() {
        let (text, report) = redact("/home/olivia/.minecraft/logs/latest.log");
        assert!(text.starts_with("/home/<user>"));
        assert_eq!(report.home_paths, 1);

        let (text, _) = redact("/Users/olivia/Library/Application Support/minecraft");
        assert!(text.starts_with("/Users/<user>"));
    }

    #[test]
    fn access_token_never_survives() {
        let (text, report) =
            redact("java -Xmx4G --accessToken eyJhbGciOiJIUzI1NiJ9.secret.value net.minecraft.client.main.Main");
        assert!(!text.contains("eyJhbGciOiJIUzI1NiJ9"));
        assert!(text.contains("--accessToken <redacted>"));
        assert_eq!(report.tokens, 1);
    }

    #[test]
    fn bearer_and_json_tokens_are_replaced() {
        let (text, report) = redact(
            "Authorization: Bearer abc.def.ghi\n{\"accessToken\":\"abcdef123456\",\"user\":\"x\"}",
        );
        assert!(!text.contains("abc.def.ghi"));
        assert!(!text.contains("abcdef123456"));
        assert!(report.tokens >= 2);
    }

    #[test]
    fn uuids_and_emails_are_replaced() {
        let (text, report) =
            redact("Player 069a79f4-44e9-4726-a5be-fca90e38aaf5 wrote to steve@example.com");
        assert!(!text.contains("069a79f4"));
        assert!(text.contains("<uuid>"));
        assert!(!text.contains("steve@example.com"));
        assert!(text.contains("<email>"));
        assert_eq!(report.uuids, 1);
        assert_eq!(report.emails, 1);
    }

    #[test]
    fn ordinary_text_is_untouched() {
        let log = "[12:00:00] [main/INFO]: Loading 64 mods\njava.lang.OutOfMemoryError: Java heap space";
        let (text, report) = redact(log);
        assert_eq!(text, log);
        assert_eq!(report.total(), 0);
    }

    #[test]
    fn unicode_text_does_not_panic() {
        let (text, _) = redact("Zażółć gęślą jaźń — C:\\Users\\olivia\\mods\\ąć.jar");
        assert!(text.contains("Zażółć"));
        assert!(text.contains("<user>"));
    }
}
