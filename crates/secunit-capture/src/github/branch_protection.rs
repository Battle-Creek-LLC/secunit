//! `secunit capture github branch-protection` — single-object endpoint.

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::canonical::{canonicalize_value, strip_keys, strip_keys_matching, Envelope};

use super::{is_url_key, GhClient, EPHEMERAL_KEYS};

pub const CAPTURER: &str = "github.branch-protection";
pub const VERSION: &str = "1";

pub async fn capture(client: &GhClient, owner: &str, repo: &str, branch: &str) -> Result<Envelope> {
    let route = format!("/repos/{owner}/{repo}/branches/{branch}/protection");
    let mut v: Value = client
        .inner
        .get::<Value, _, ()>(&route, None)
        .await
        .with_context(|| format!("GET {route}"))?;
    strip_keys(&mut v, EPHEMERAL_KEYS);
    strip_keys_matching(&mut v, is_url_key);

    Ok(Envelope::new(
        CAPTURER,
        VERSION,
        json!({
            "repo": format!("{owner}/{repo}"),
            "branch": branch,
        }),
        canonicalize_value(v),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn server() -> MockServer {
        let s = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/o/r/branches/main/protection"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "url": "https://api.example/x",
                "required_pull_request_reviews": {"dismiss_stale_reviews": true},
                "required_status_checks": {"strict": true, "contexts": ["c", "a", "b"]},
                "enforce_admins": {"enabled": true, "url": "https://x"}
            })))
            .mount(&s)
            .await;
        s
    }

    #[tokio::test]
    async fn branch_protection_canonicalizes() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let s = server().await;
        let c = GhClient::with_base_uri(&s.uri(), Some("ghp_x")).unwrap();
        let env = capture(&c, "o", "r", "main").await.unwrap();
        let body = env.to_canonical_json().unwrap();
        assert!(!body.contains("\"url\""));
        // keys are sorted alphabetically — `enforce_admins` before
        // `required_pull_request_reviews` before `required_status_checks`.
        let ea = body.find("enforce_admins").unwrap();
        let rpr = body.find("required_pull_request_reviews").unwrap();
        let rsc = body.find("required_status_checks").unwrap();
        assert!(ea < rpr && rpr < rsc);
    }

    #[tokio::test]
    async fn branch_protection_byte_identical() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let s = server().await;
        let c = GhClient::with_base_uri(&s.uri(), Some("ghp_x")).unwrap();
        let a = capture(&c, "o", "r", "main")
            .await
            .unwrap()
            .to_canonical_json()
            .unwrap();
        let b = capture(&c, "o", "r", "main")
            .await
            .unwrap()
            .to_canonical_json()
            .unwrap();
        assert_eq!(a, b);
    }
}
