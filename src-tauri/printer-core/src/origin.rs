//! Browser Origin checks for the localhost APIs (desktop device API and the
//! legacy helper), so random websites can't drive the printer or drawer.

/// Websites allowed by default: the POS itself and local development.
pub const DEFAULT_ALLOWED_ORIGINS: &[&str] = &[
    "https://*.reliatech.co.ke",
    "http://localhost",
    "http://localhost:*",
    "http://127.0.0.1",
    "http://127.0.0.1:*",
];

/// Glob match where `*` matches any run of characters.
fn glob_match(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut pi, mut ti, mut star, mut mark) = (0, 0, None, 0);
    while ti < t.len() {
        if pi < p.len() && p[pi] != '*' && p[pi].eq_ignore_ascii_case(&t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

/// Requests without an Origin header (native apps, same-origin GETs) are
/// always allowed; browsers always send one on cross-site requests.
pub fn origin_allowed(origin: Option<&str>, allowed: &[String]) -> bool {
    match origin {
        None => true,
        Some(o) => allowed.iter().any(|p| p == "*" || glob_match(p.trim().trim_end_matches('/'), o)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_rules() {
        let allowed: Vec<String> = DEFAULT_ALLOWED_ORIGINS.iter().map(|s| s.to_string()).collect();
        assert!(origin_allowed(None, &allowed));
        assert!(origin_allowed(Some("https://pos.reliatech.co.ke"), &allowed));
        assert!(origin_allowed(Some("https://a.b.reliatech.co.ke"), &allowed));
        assert!(origin_allowed(Some("http://localhost:5173"), &allowed));
        assert!(origin_allowed(Some("http://localhost"), &allowed));
        assert!(origin_allowed(Some("http://127.0.0.1:14200"), &allowed));
        assert!(!origin_allowed(Some("https://evilreliatech.co.ke"), &allowed));
        assert!(!origin_allowed(Some("https://reliatech.co.ke.evil.com"), &allowed));
        assert!(!origin_allowed(Some("http://localhost.evil.com"), &allowed));
        assert!(!origin_allowed(Some("null"), &allowed));
        assert!(origin_allowed(Some("https://anything"), &["*".to_string()]));
    }
}
