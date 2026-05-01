//! `secunit capture github codeql-alerts` — list code-scanning alerts
//! (CodeQL or third-party tooling) for a repo.

use anyhow::Result;
use serde_json::json;

use crate::canonical::{
    canonicalize_value, sort_array_by_key, strip_keys, strip_keys_matching, Envelope,
};

use super::{is_url_key, paginate_array, GhClient, EPHEMERAL_KEYS};

pub const CAPTURER: &str = "github.codeql-alerts";
pub const VERSION: &str = "1";

pub async fn capture(client: &GhClient, owner: &str, repo: &str) -> Result<Envelope> {
    let route = format!("/repos/{owner}/{repo}/code-scanning/alerts");
    let mut alerts = paginate_array(client, &route).await?;
    for a in alerts.iter_mut() {
        strip_keys(a, EPHEMERAL_KEYS);
        strip_keys_matching(a, is_url_key);
    }
    sort_array_by_key(&mut alerts, "number");
    let result = canonicalize_value(json!({ "alerts": alerts }));

    Ok(Envelope::new(
        CAPTURER,
        VERSION,
        json!({ "repo": format!("{owner}/{repo}") }),
        result,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn server() -> MockServer {
        let s = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/o/r/code-scanning/alerts"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {
                    "number": 5,
                    "state": "open",
                    "rule": {"id": "rust/unsafe", "severity": "warning"},
                    "url": "https://x"
                },
                {
                    "number": 3,
                    "state": "dismissed",
                    "rule": {"id": "py/eval", "severity": "error"}
                }
            ])))
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/o/r/code-scanning/alerts"))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&s)
            .await;
        s
    }

    #[tokio::test]
    async fn codeql_alerts_canonicalizes() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let s = server().await;
        let c = GhClient::with_base_uri(&s.uri(), Some("ghp_x")).unwrap();
        let env = capture(&c, "o", "r").await.unwrap();
        let body = env.to_canonical_json().unwrap();
        let p3 = body.find("\"number\": 3").unwrap();
        let p5 = body.find("\"number\": 5").unwrap();
        assert!(p3 < p5);
        assert!(!body.contains("\"url\""));
    }

    #[tokio::test]
    async fn codeql_alerts_byte_identical() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let s = server().await;
        let c = GhClient::with_base_uri(&s.uri(), Some("ghp_x")).unwrap();
        let a = capture(&c, "o", "r")
            .await
            .unwrap()
            .to_canonical_json()
            .unwrap();
        let b = capture(&c, "o", "r")
            .await
            .unwrap()
            .to_canonical_json()
            .unwrap();
        assert_eq!(a, b);
    }
}
