use std::net::Ipv4Addr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum NormalizedValue {
    Ip(Ipv4Addr),
    IpList(Vec<Ipv4Addr>),
    Bool(bool),
    Int(i64),
    String(String),
    Hex(String),
    Opaque(String),
}

impl NormalizedValue {
    pub fn from_raw_text(raw: &str) -> Self {
        let trimmed = raw.trim().trim_end_matches(';').trim();
        if trimmed.eq_ignore_ascii_case("true") || trimmed.eq_ignore_ascii_case("on") {
            return Self::Bool(true);
        }
        if trimmed.eq_ignore_ascii_case("false") || trimmed.eq_ignore_ascii_case("off") {
            return Self::Bool(false);
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let hex = trimmed
                .trim_start_matches('[')
                .trim_end_matches(']')
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
                .to_ascii_lowercase();
            return Self::Hex(hex);
        }
        if let Ok(ip) = trimmed.parse::<Ipv4Addr>() {
            return Self::Ip(ip);
        }
        if let Ok(n) = trimmed.parse::<i64>() {
            return Self::Int(n);
        }
        if trimmed.contains(',') {
            let ips: Vec<Ipv4Addr> = trimmed
                .split(',')
                .filter_map(|p| p.trim().parse().ok())
                .collect();
            if !ips.is_empty() && ips.len() == trimmed.split(',').count() {
                return Self::IpList(ips);
            }
        }
        let looks_quoted = trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2;
        if looks_quoted {
            let interior = &trimmed[1..trimmed.len() - 1];
            return Self::String(unescape_isc_string(interior));
        }
        if let Some(hex) = parse_colon_hex(trimmed) {
            return Self::Hex(hex);
        }
        Self::String(trimmed.to_string())
    }

    pub fn sort_ip_list_if_needed(&mut self) {
        if let Self::IpList(ips) = self {
            ips.sort();
            ips.dedup();
        }
    }
}

/// Decode ISC dhcpd.conf C-style escapes inside a quoted string (interior only).
///
/// Matches ISC `common/conflex.c` / dhcp-eval: `\t` `\r` `\n` `\b`, `\xNN`,
/// octal `\ddd`, and any other `\X` → literal `X`.
pub fn unescape_isc_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            None => break,
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('n') => out.push('\n'),
            Some('b') => out.push('\u{0008}'),
            Some('x') | Some('X') => {
                let mut value = 0u32;
                let mut digits = 0u8;
                while digits < 2 {
                    match chars.peek().copied() {
                        Some(d) if d.is_ascii_hexdigit() => {
                            chars.next();
                            value = value * 16 + d.to_digit(16).unwrap();
                            digits += 1;
                        }
                        _ => break,
                    }
                }
                if digits > 0 {
                    push_byte(&mut out, value);
                } else {
                    out.push('x');
                }
            }
            Some(d) if ('0'..='7').contains(&d) => {
                let mut value = d.to_digit(8).unwrap();
                let mut digits = 1u8;
                while digits < 3 {
                    match chars.peek().copied() {
                        Some(n) if ('0'..='7').contains(&n) => {
                            chars.next();
                            value = value * 8 + n.to_digit(8).unwrap();
                            digits += 1;
                        }
                        _ => break,
                    }
                }
                push_byte(&mut out, value & 0xff);
            }
            Some(other) => out.push(other),
        }
    }
    out
}

fn push_byte(out: &mut String, value: u32) {
    // DHCP option payloads are octets; map each decoded byte into a char.
    out.push(char::from_u32(value).unwrap_or('\u{FFFD}'));
}

/// Find the first `"..."` in `s`, return decoded interior and byte index after the closing `"`.
pub fn extract_quoted_isc(s: &str) -> Option<(String, usize)> {
    let start = s.find('"')?;
    let b = s.as_bytes();
    let mut i = start + 1;
    while i < b.len() {
        match b[i] {
            b'"' => {
                let interior = &s[start + 1..i];
                return Some((unescape_isc_string(interior), i + 1));
            }
            b'\\' if i + 1 < b.len() => {
                i += 1;
                match b[i] {
                    b'x' | b'X' => {
                        i += 1;
                        let mut digits = 0u8;
                        while digits < 2 && i < b.len() && b[i].is_ascii_hexdigit() {
                            i += 1;
                            digits += 1;
                        }
                    }
                    b'0'..=b'7' => {
                        i += 1;
                        let mut digits = 1u8;
                        while digits < 3 && i < b.len() && (b'0'..=b'7').contains(&b[i]) {
                            i += 1;
                            digits += 1;
                        }
                    }
                    _ => {
                        i += 1;
                    }
                }
            }
            _ => i += 1,
        }
    }
    None
}

/// ISC string/data option form: colon-separated hex octets (1–2 digits each).
fn parse_colon_hex(s: &str) -> Option<String> {
    if !s.contains(':') {
        return None;
    }
    let parts: Vec<&str> = s.split(':').map(str::trim).collect();
    if parts.len() < 2 {
        return None;
    }
    if !parts.iter().all(|p| {
        (1..=2).contains(&p.len()) && p.chars().all(|c| c.is_ascii_hexdigit())
    }) {
        return None;
    }
    Some(
        parts
            .iter()
            .map(|p| p.to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join(":"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colon_hex_case_insensitive() {
        let upper = NormalizedValue::from_raw_text("AA:BB:CC");
        let lower = NormalizedValue::from_raw_text("aa:bb:cc");
        assert_eq!(upper, lower);
        assert_eq!(upper, NormalizedValue::Hex("aa:bb:cc".into()));
    }

    #[test]
    fn colon_hex_mixed_case_and_single_digit() {
        assert_eq!(
            NormalizedValue::from_raw_text("A:b:0C"),
            NormalizedValue::Hex("a:b:0c".into())
        );
    }

    #[test]
    fn quoted_colon_text_stays_string() {
        assert_eq!(
            NormalizedValue::from_raw_text("\"AA:BB\""),
            NormalizedValue::String("AA:BB".into())
        );
    }

    #[test]
    fn bracket_hex_still_lowercased() {
        assert_eq!(
            NormalizedValue::from_raw_text("[AA BB CC]"),
            NormalizedValue::Hex("aabbcc".into())
        );
    }

    #[test]
    fn bootfile_backslash_escapes_are_equivalent() {
        let via_double = NormalizedValue::from_raw_text(r#""SMSBoot\\x86\\wdsnbp.com""#);
        let via_hex = NormalizedValue::from_raw_text(r#""SMSBoot\x5cx86\x5cwdsnbp.com""#);
        let via_octal = NormalizedValue::from_raw_text(r#""SMSBoot\134x86\134wdsnbp.com""#);
        let expected = NormalizedValue::String(r"SMSBoot\x86\wdsnbp.com".into());
        assert_eq!(via_double, expected);
        assert_eq!(via_hex, expected);
        assert_eq!(via_octal, expected);
        assert_eq!(via_double, via_hex);
        assert_eq!(via_hex, via_octal);
    }

    #[test]
    fn common_and_unknown_escapes() {
        assert_eq!(
            NormalizedValue::from_raw_text(r#""a\tb\nc""#),
            NormalizedValue::String("a\tb\nc".into())
        );
        assert_eq!(
            NormalizedValue::from_raw_text(r#""\x41\101""#),
            NormalizedValue::String("AA".into())
        );
        assert_eq!(
            NormalizedValue::from_raw_text(r#""\q""#),
            NormalizedValue::String("q".into())
        );
    }

    #[test]
    fn unquoted_colon_hex_not_unescaped() {
        assert_eq!(
            NormalizedValue::from_raw_text(r"aa:bb"),
            NormalizedValue::Hex("aa:bb".into())
        );
    }

    #[test]
    fn extract_quoted_isc_decodes_and_reports_end() {
        let (decoded, end) = extract_quoted_isc(r#"prefix "a\\b\x5cc" suffix"#).unwrap();
        assert_eq!(decoded, r"a\b\c");
        assert_eq!(s_after(r#"prefix "a\\b\x5cc" suffix"#, end), " suffix");
    }

    #[test]
    fn extract_quoted_isc_respects_escaped_quote() {
        let (decoded, end) = extract_quoted_isc(r#""say \"hi\"""#).unwrap();
        assert_eq!(decoded, r#"say "hi""#);
        assert_eq!(end, r#""say \"hi\"""#.len());
    }

    fn s_after(s: &str, end: usize) -> &str {
        &s[end..]
    }
}
