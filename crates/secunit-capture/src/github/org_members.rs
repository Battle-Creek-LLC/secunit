//! `secunit capture github org-members` — list organization members.

use anyhow::Result;
use serde_json::json;

use crate::canonical::{
    canonicalize_value, sort_array_by_key, strip_keys, strip_keys_matching, Envelope,
};

use super::{is_url_key, paginate_array, GhClient, EPHEMERAL_KEYS};

pub const CAPTURER: &str = "github.org-members";
pub const VERSION: &str = "1";

pub async fn capture(client: &GhClient, org: &str) -> Result<Envelope> {
    let route = format!("/orgs/{org}/members");
    let mut members = paginate_array(client, &route).await?;
    for m in members.iter_mut() {
        strip_keys(m, EPHEMERAL_KEYS);
        strip_keys_matching(m, is_url_key);
    }
    sort_array_by_key(&mut members, "login");
    let result = canonicalize_value(json!({ "members": members }));

    Ok(Envelope::new(
        CAPTURER,
        VERSION,
        json!({ "org": org }),
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
            .and(path("/orgs/acme/members"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {
                    "login": "zach",
                    "id": 3,
                    "node_id": "MDz",
                    "avatar_url": "https://x",
                    "type": "User"
                },
                {
                    "login": "alice",
                    "id": 1,
                    "node_id": "MDa",
                    "avatar_url": "https://y",
                    "type": "User"
                }
            ])))
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/acme/members"))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&s)
            .await;
        s
    }

    #[tokio::test]
    async fn org_members_canonicalizes_and_sorts_by_login() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let s = server().await;
        let c = GhClient::with_base_uri(&s.uri(), Some("ghp_x")).unwrap();
        let env = capture(&c, "acme").await.unwrap();
        let body = env.to_canonical_json().unwrap();
        let pa = body.find("alice").unwrap();
        let pz = body.find("zach").unwrap();
        assert!(pa < pz);
        assert!(!body.contains("avatar_url"));
        assert!(!body.contains("node_id"));
    }

    #[tokio::test]
    async fn org_members_byte_identical() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let s = server().await;
        let c = GhClient::with_base_uri(&s.uri(), Some("ghp_x")).unwrap();
        let a = capture(&c, "acme")
            .await
            .unwrap()
            .to_canonical_json()
            .unwrap();
        let b = capture(&c, "acme")
            .await
            .unwrap()
            .to_canonical_json()
            .unwrap();
        assert_eq!(a, b);
    }
}
