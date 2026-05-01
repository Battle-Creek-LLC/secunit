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

/// Walk every page of a paginated list endpoint.
///
/// Pagination is driven by `?per_page=100&page=N` until GitHub returns
/// an empty array. We deliberately ignore the `Link` header — pages
/// are stable, and the explicit `page` counter makes test fixtures
/// easier to record.
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
