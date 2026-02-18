use axum::http::HeaderMap;

const GH_TOKEN_PREFIXES: &[&str] = &["ghp_", "gho_", "ghu_", "github_pat_"];

fn looks_like_gh_token(s: &str) -> bool {
    GH_TOKEN_PREFIXES.iter().any(|p| s.starts_with(p))
}

/// Try to extract a GitHub token from request headers.
///
/// Checks (in order):
/// 1. `x-api-key` (Anthropic convention)
/// 2. `authorization: Bearer <token>` (OpenAI / general)
/// 3. `api-key` (Azure convention)
///
/// Only returns the value if it looks like a GitHub token (known prefix).
pub fn extract_gh_token(headers: &HeaderMap) -> Option<&str> {
    // x-api-key (Anthropic)
    if let Some(val) = header_str(headers, "x-api-key")
        && looks_like_gh_token(val)
    {
        return Some(val);
    }

    // Authorization: Bearer ... (OpenAI / generic)
    if let Some(val) = header_str(headers, "authorization") {
        let token = val
            .strip_prefix("Bearer ")
            .or_else(|| val.strip_prefix("bearer "));
        if let Some(token) = token
            && looks_like_gh_token(token)
        {
            return Some(token);
        }
    }

    // api-key (Azure)
    if let Some(val) = header_str(headers, "api-key")
        && looks_like_gh_token(val)
    {
        return Some(val);
    }

    None
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, HeaderValue};

    #[test]
    fn anthropic_x_api_key() {
        let mut h = HeaderMap::new();
        h.insert("x-api-key", HeaderValue::from_static("ghp_abc123"));
        assert_eq!(extract_gh_token(&h), Some("ghp_abc123"));
    }

    #[test]
    fn openai_bearer() {
        let mut h = HeaderMap::new();
        h.insert(
            "authorization",
            HeaderValue::from_static("Bearer gho_token123"),
        );
        assert_eq!(extract_gh_token(&h), Some("gho_token123"));
    }

    #[test]
    fn bearer_lowercase() {
        let mut h = HeaderMap::new();
        h.insert("authorization", HeaderValue::from_static("bearer ghp_low"));
        assert_eq!(extract_gh_token(&h), Some("ghp_low"));
    }

    #[test]
    fn azure_api_key() {
        let mut h = HeaderMap::new();
        h.insert("api-key", HeaderValue::from_static("github_pat_foobar"));
        assert_eq!(extract_gh_token(&h), Some("github_pat_foobar"));
    }

    #[test]
    fn non_gh_token_ignored() {
        let mut h = HeaderMap::new();
        h.insert(
            "x-api-key",
            HeaderValue::from_static("sk-ant-api03-something"),
        );
        h.insert(
            "authorization",
            HeaderValue::from_static("Bearer sk-proj-something"),
        );
        assert_eq!(extract_gh_token(&h), None);
    }

    #[test]
    fn no_headers() {
        let h = HeaderMap::new();
        assert_eq!(extract_gh_token(&h), None);
    }

    #[test]
    fn x_api_key_takes_priority() {
        let mut h = HeaderMap::new();
        h.insert("x-api-key", HeaderValue::from_static("ghp_first"));
        h.insert(
            "authorization",
            HeaderValue::from_static("Bearer gho_second"),
        );
        assert_eq!(extract_gh_token(&h), Some("ghp_first"));
    }

    #[test]
    fn ghu_prefix_accepted() {
        let mut h = HeaderMap::new();
        h.insert("x-api-key", HeaderValue::from_static("ghu_usertoken"));
        assert_eq!(extract_gh_token(&h), Some("ghu_usertoken"));
    }
}
