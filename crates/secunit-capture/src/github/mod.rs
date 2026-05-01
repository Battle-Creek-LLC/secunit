//! GitHub capturers using `octocrab` for auth + transport, plain
//! `serde_json::Value` for response shape so we can canonicalize freely
//! without locking onto octocrab's typed models.

pub mod audit_log;
pub mod branch_protection;
pub mod codeql_alerts;
pub mod dependabot_alerts;
pub mod org_members;

use anyhow::{anyhow, Context, Result};
use octocrab::Octocrab;
use serde_json::Value;

/// Keys we always strip from GitHub responses before canonicalization.
/// These either contain ephemeral URLs that change with API rev, or
/// are identifiers volatile across the same logical record.
pub(crate) const EPHEMERAL_KEYS: &[&str] = &["node_id", "etag", "_links"];

/// Suffixes that mark URL fields. Stripped via `strip_keys_matching`.
pub(crate) fn is_url_key(k: &str) -> bool {
    k == "url" || k == "html_url" || k == "avatar_url" || k == "gravatar_id" || k.ends_with("_url")
}

/// Wrapper around an authenticated `Octocrab` client. Built either
/// from `GITHUB_TOKEN` (production) or from a wiremock URI (tests).
pub struct GhClient {
    pub(crate) inner: Octocrab,
}

impl GhClient {
    /// Build from the standard credential chain. Returns an error
    /// (mapped to exit 2 by callers) if `GITHUB_TOKEN` is unset.
    pub fn from_env() -> Result<Self> {
        let token =
            std::env::var("GITHUB_TOKEN").map_err(|_| anyhow!("GITHUB_TOKEN is not set"))?;
        let inner = Octocrab::builder()
            .personal_token(token)
            .build()
            .context("build octocrab client")?;
        Ok(Self { inner })
    }

    /// Build pointing at an arbitrary base URI (used by tests with
    /// wiremock).
    pub fn with_base_uri(base_uri: &str, token: Option<&str>) -> Result<Self> {
        let mut b = Octocrab::builder();
        if let Some(t) = token {
            b = b.personal_token(t.to_string());
        }
        let inner = b
            .base_uri(base_uri)
            .context("invalid base_uri")?
            .build()
            .context("build octocrab client")?;
        Ok(Self { inner })
    }
}

/// Walk every page of a paginated list endpoint using `?page=N` query
/// pagination.
///
/// Pagination is driven by `?per_page=100&page=N` until GitHub returns
/// an empty array. We deliberately ignore the `Link` header for the
/// page-based endpoints — pages are stable, and the explicit `page`
/// counter makes test fixtures easier to record.
///
/// **Do not use for newer endpoints that require cursor pagination**
/// (e.g. dependabot alerts). They reject `page=N` with HTTP 422
/// "Pagination using the `page` parameter is not supported." Use
/// [`paginate_array_cursor`] instead.
pub(crate) async fn paginate_array(
    client: &GhClient,
    route_with_query: &str,
) -> Result<Vec<Value>> {
    let sep = if route_with_query.contains('?') {
        '&'
    } else {
        '?'
    };
    let mut out = Vec::new();
    let mut page: u32 = 1;
    loop {
        let url = format!("{route_with_query}{sep}per_page=100&page={page}");
        let v: Value = client
            .inner
            .get::<Value, _, ()>(&url, None)
            .await
            .with_context(|| format!("GET {url}"))?;
        let arr = match v {
            Value::Array(a) => a,
            other => return Err(anyhow!("expected array from {url}, got {other:?}")),
        };
        if arr.is_empty() {
            break;
        }
        let was_full = arr.len() == 100;
        out.extend(arr);
        if !was_full {
            break;
        }
        page += 1;
        if page > 1000 {
            return Err(anyhow!(
                "pagination cutoff exceeded for {route_with_query} (>100k records)"
            ));
        }
    }
    Ok(out)
}

/// Walk every page of a list endpoint that uses cursor pagination via
/// the `Link` header (e.g. `/repos/{o}/{r}/dependabot/alerts`).
///
/// First request appends `per_page=100` to the supplied route. Each
/// response's `Link` header is parsed for a `rel="next"` URL; if
/// present, that URL is fetched verbatim (it carries the opaque cursor
/// in `before=`/`after=`). Iteration stops when no `next` link is
/// returned.
pub(crate) async fn paginate_array_cursor(
    client: &GhClient,
    route_with_query: &str,
) -> Result<Vec<Value>> {
    let sep = if route_with_query.contains('?') {
        '&'
    } else {
        '?'
    };
    let mut url = format!("{route_with_query}{sep}per_page=100");
    let mut out = Vec::new();
    for _ in 0..1000 {
        let resp = client
            .inner
            ._get(url.as_str())
            .await
            .with_context(|| format!("GET {url}"))?;
        let resp = octocrab::map_github_error(resp)
            .await
            .with_context(|| format!("GET {url}"))?;
        let next = resp
            .headers()
            .get("Link")
            .and_then(|v| v.to_str().ok())
            .and_then(parse_link_header_next);
        let body = client
            .inner
            .body_to_string(resp)
            .await
            .with_context(|| format!("read body of {url}"))?;
        let v: Value =
            serde_json::from_str(&body).with_context(|| format!("parse JSON body of {url}"))?;
        let arr = match v {
            Value::Array(a) => a,
            other => return Err(anyhow!("expected array from {url}, got {other:?}")),
        };
        out.extend(arr);
        match next {
            Some(n) => url = n,
            None => return Ok(out),
        }
    }
    Err(anyhow!(
        "cursor pagination cutoff exceeded for {route_with_query} (>1000 pages)"
    ))
}

/// Extract the URL of the `rel="next"` entry from a GitHub `Link`
/// header value. Returns `None` if the header has no next link or
/// cannot be parsed.
fn parse_link_header_next(link_header: &str) -> Option<String> {
    for part in link_header.split(',') {
        let mut it = part.split(';');
        let url = it.next()?.trim();
        let url = url.strip_prefix('<')?.strip_suffix('>')?;
        for param in it {
            let param = param.trim();
            let (k, v) = match param.split_once('=') {
                Some(kv) => kv,
                None => continue,
            };
            if k.trim() == "rel" && v.trim().trim_matches('"') == "next" {
                return Some(url.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::parse_link_header_next;

    #[test]
    fn parse_link_extracts_next() {
        let h = r#"<https://api.github.com/repos/o/r/dependabot/alerts?per_page=100&after=CUR>; rel="next", <https://api.github.com/repos/o/r/dependabot/alerts?per_page=100&after=LAST>; rel="last""#;
        assert_eq!(
            parse_link_header_next(h).as_deref(),
            Some("https://api.github.com/repos/o/r/dependabot/alerts?per_page=100&after=CUR"),
        );
    }

    #[test]
    fn parse_link_returns_none_when_only_prev_or_last() {
        let h = r#"<https://api.github.com/x?page=1>; rel="first", <https://api.github.com/x?page=4>; rel="last""#;
        assert_eq!(parse_link_header_next(h), None);
    }

    #[test]
    fn parse_link_returns_none_for_empty() {
        assert_eq!(parse_link_header_next(""), None);
    }

    #[test]
    fn parse_link_handles_extra_params() {
        let h = r#"<https://api.github.com/x?after=Z>; foo="bar"; rel="next""#;
        assert_eq!(
            parse_link_header_next(h).as_deref(),
            Some("https://api.github.com/x?after=Z"),
        );
    }
}
