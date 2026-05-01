//! `secunit capture github audit-log` — org audit log entries since
//! a relative duration ago.

use anyhow::Result;
use serde_json::json;

use crate::canonical::{
    canonicalize_value, sort_array_by_key, strip_keys, strip_keys_matching, Envelope,
};

use super::{is_url_key, paginate_array, GhClient, EPHEMERAL_KEYS};

pub const CAPTURER: &str = "github.audit-log";
pub const VERSION: &str = "1";

/// Capture audit log entries for `org` matching events recorded since
/// `since_iso8601`. The `since` parameter is the operator-supplied
/// duration window already resolved to an absolute timestamp.
pub async fn capture(client: &GhClient, org: &str, since_iso8601: &str) -> Result<Envelope> {
    let phrase = format!("created:>={since_iso8601}");
    let route = format!(
        "/orgs/{org}/audit-log?phrase={}",
        urlencoding::encode(&phrase)
    );
    let mut entries = paginate_array(client, &route).await?;
    for e in entries.iter_mut() {
        // The audit-log "_document_id" is per-event but stable — keep
        // it. Strip the volatile @ timestamp_milliseconds key in favor
        // of the canonical "@timestamp" field.
        strip_keys(e, EPHEMERAL_KEYS);
        strip_keys(
            e,
            &["external_identity_username", "external_identity_nameid"],
        );
        strip_keys_matching(e, is_url_key);
    }
    // Audit log is naturally chronological (newest first from GitHub).
    // Sort by (@timestamp, _document_id) ascending so the canonical
    // form is stable regardless of upstream order or pagination
    // boundary effects.
    sort_array_by_key(&mut entries, "@timestamp");

    let result = canonicalize_value(json!({
        "since": since_iso8601,
        "entries": entries,
    }));

    Ok(Envelope::new(
        CAPTURER,
        VERSION,
        json!({ "org": org, "since": since_iso8601 }),
        result,
    ))
}

mod urlencoding {
    /// Tiny URL-encoder for the `phrase=` query value. Escapes anything
    /// outside `A-Za-z0-9_.-`. Keeps the dependency surface small.
    pub fn encode(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => out.push(b as char),
                _ => out.push_str(&format!("%{:02X}", b)),
            }
        }
        out
    }
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
            .and(path("/orgs/acme/audit-log"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {
                    "@timestamp": 1714521600,
                    "_document_id": "doc-2",
                    "action": "repo.create",
                    "actor": "bob"
                },
                {
                    "@timestamp": 1714435200,
                    "_document_id": "doc-1",
                    "action": "team.add_member",
                    "actor": "alice"
                }
            ])))
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/acme/audit-log"))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&s)
            .await;
        s
    }

    #[tokio::test]
    async fn audit_log_sorts_by_timestamp_ascending() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let s = server().await;
        let c = GhClient::with_base_uri(&s.uri(), Some("ghp_x")).unwrap();
        let env = capture(&c, "acme", "2026-04-01T00:00:00Z").await.unwrap();
        let body = env.to_canonical_json().unwrap();
        let p1 = body.find("doc-1").unwrap();
        let p2 = body.find("doc-2").unwrap();
        assert!(p1 < p2, "older event must come first");
    }

    #[tokio::test]
    async fn audit_log_byte_identical() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let s = server().await;
        let c = GhClient::with_base_uri(&s.uri(), Some("ghp_x")).unwrap();
        let a = capture(&c, "acme", "2026-04-01T00:00:00Z")
            .await
            .unwrap()
            .to_canonical_json()
            .unwrap();
        let b = capture(&c, "acme", "2026-04-01T00:00:00Z")
            .await
            .unwrap()
            .to_canonical_json()
            .unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn urlencode_basics() {
        assert_eq!(
            urlencoding::encode("created:>=2026-04"),
            "created%3A%3E%3D2026-04"
        );
    }
}
