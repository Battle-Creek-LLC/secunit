//! `secunit capture github dependabot-alerts` — list all Dependabot
//! alerts for a repo.

use anyhow::Result;
use serde_json::{json, Value};

use crate::canonical::{
    canonicalize_value, sort_array_by_key, strip_keys, strip_keys_matching, Envelope,
};

use super::{is_url_key, paginate_array, GhClient, EPHEMERAL_KEYS};

pub const CAPTURER: &str = "github.dependabot-alerts";
pub const VERSION: &str = "1";

pub async fn capture(
    client: &GhClient,
    owner: &str,
    repo: &str,
    state: Option<&str>,
) -> Result<Envelope> {
    let route = match state {
        Some(s) => format!("/repos/{owner}/{repo}/dependabot/alerts?state={s}"),
        None => format!("/repos/{owner}/{repo}/dependabot/alerts"),
    };
    let mut alerts = paginate_array(client, &route).await?;
    let result = canonicalize(&mut alerts);

    Ok(Envelope::new(
        CAPTURER,
        VERSION,
        json!({
            "repo": format!("{owner}/{repo}"),
            "state": state.unwrap_or("all"),
        }),
        result,
    ))
}

fn canonicalize(alerts: &mut Vec<Value>) -> Value {
    for a in alerts.iter_mut() {
        strip_keys(a, EPHEMERAL_KEYS);
        strip_keys_matching(a, is_url_key);
    }
    sort_array_by_key(alerts, "number");
    let arr = std::mem::take(alerts);
    canonicalize_value(json!({ "alerts": arr }))
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
            .and(path("/repos/o/r/dependabot/alerts"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {
                    "number": 7,
                    "state": "open",
                    "url": "https://api.example/x",
                    "html_url": "https://example/x",
                    "node_id": "MDx",
                    "security_advisory": {"ghsa_id": "GHSA-x", "summary": "Z"},
                    "dependency": {"package": {"name": "lodash"}}
                },
                {
                    "number": 2,
                    "state": "open",
                    "url": "https://api.example/y",
                    "node_id": "MDy",
                    "security_advisory": {"ghsa_id": "GHSA-y", "summary": "A"},
                    "dependency": {"package": {"name": "axios"}}
                }
            ])))
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/o/r/dependabot/alerts"))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&s)
            .await;
        s
    }

    #[tokio::test]
    async fn dependabot_canonicalizes() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let s = server().await;
        let c = GhClient::with_base_uri(&s.uri(), Some("ghp_x")).unwrap();
        let env = capture(&c, "o", "r", None).await.unwrap();
        let body = env.to_canonical_json().unwrap();
        // url stripping
        assert!(!body.contains("html_url"));
        assert!(!body.contains("\"url\""));
        assert!(!body.contains("node_id"));
        // number sorting (2 < 7)
        let p2 = body.find("\"number\": 2").unwrap();
        let p7 = body.find("\"number\": 7").unwrap();
        assert!(p2 < p7);
    }

    #[tokio::test]
    async fn dependabot_byte_identical() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let s = server().await;
        let c = GhClient::with_base_uri(&s.uri(), Some("ghp_x")).unwrap();
        let a = capture(&c, "o", "r", None)
            .await
            .unwrap()
            .to_canonical_json()
            .unwrap();
        let b = capture(&c, "o", "r", None)
            .await
            .unwrap()
            .to_canonical_json()
            .unwrap();
        assert_eq!(a, b);
    }
}
