//! OSV.dev REST query capturer. POSTs a single
//! `{ package: { ecosystem, name }, version }` query and canonicalizes
//! the returned `vulns[]` array.

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

use crate::canonical::{
    canonicalize_value, sort_array_by_key, strip_keys, strip_keys_matching, Envelope,
};

pub const CAPTURER: &str = "deps.osv-query";
pub const VERSION: &str = "1";

const DEFAULT_OSV_BASE: &str = "https://api.osv.dev";

pub struct OsvArgs<'a> {
    pub ecosystem: &'a str,
    pub package: &'a str,
    pub version: &'a str,
}

/// Capture OSV findings for a single package@version.
pub async fn capture(args: OsvArgs<'_>) -> Result<Envelope> {
    let base = std::env::var("SECUNIT_OSV_BASE").unwrap_or_else(|_| DEFAULT_OSV_BASE.to_string());
    capture_with_base(&base, args).await
}

/// Same as [`capture`] with an explicit base URL (test seam).
pub async fn capture_with_base(base: &str, args: OsvArgs<'_>) -> Result<Envelope> {
    let url = format!("{}/v1/query", base.trim_end_matches('/'));
    let body = json!({
        "package": { "ecosystem": args.ecosystem, "name": args.package },
        "version": args.version,
    });

    let client = reqwest::Client::builder()
        .user_agent(concat!("secunit/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("build reqwest client")?;

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .with_context(|| format!("POST {url}"))?;

    let status = resp.status();
    let text = resp.text().await.context("read OSV response body")?;
    if !status.is_success() {
        return Err(anyhow!(
            "OSV {url} returned HTTP {status}: {}",
            truncate(&text, 200)
        ));
    }
    let raw: Value = serde_json::from_str(&text)
        .with_context(|| format!("parse OSV response: {}", truncate(&text, 200)))?;

    Ok(Envelope::new(
        CAPTURER,
        VERSION,
        json!({
            "ecosystem": args.ecosystem,
            "package": args.package,
            "version": args.version,
        }),
        canonicalize_osv(raw),
    ))
}

fn canonicalize_osv(mut v: Value) -> Value {
    // OSV responses can include a `next_page_token` cursor, plus
    // per-vuln `database_specific` blobs that often carry source urls.
    strip_keys(&mut v, &["next_page_token"]);
    strip_keys_matching(&mut v, |k| k.ends_with("_url") && k != "purl");

    if let Some(vulns) = v.get_mut("vulns").and_then(|x| x.as_array_mut()) {
        for vuln in vulns.iter_mut() {
            if let Some(aliases) = vuln.get_mut("aliases").and_then(|a| a.as_array_mut()) {
                aliases.sort_by(|a, b| a.as_str().unwrap_or("").cmp(b.as_str().unwrap_or("")));
            }
            if let Some(refs) = vuln.get_mut("references").and_then(|r| r.as_array_mut()) {
                sort_array_by_key(refs, "url");
            }
            if let Some(affected) = vuln.get_mut("affected").and_then(|a| a.as_array_mut()) {
                for entry in affected.iter_mut() {
                    if let Some(versions) = entry.get_mut("versions").and_then(|v| v.as_array_mut())
                    {
                        versions
                            .sort_by(|a, b| a.as_str().unwrap_or("").cmp(b.as_str().unwrap_or("")));
                    }
                }
            }
        }
        sort_array_by_key(vulns, "id");
    }
    canonicalize_value(v)
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}...", &s[..n])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn fixture_server() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/query"))
            .and(body_partial_json(json!({
                "package": {"ecosystem": "PyPI", "name": "requests"}
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "vulns": [
                    {
                        "id": "GHSA-9wx4-h78v-vm56",
                        "summary": "Z thing",
                        "aliases": ["CVE-2024-35195", "PYSEC-2024-1"],
                        "references": [
                            {"type": "WEB", "url": "https://b.example"},
                            {"type": "WEB", "url": "https://a.example"}
                        ],
                        "affected": [{
                            "package": {"ecosystem": "PyPI", "name": "requests"},
                            "versions": ["2.30.0", "2.29.0"]
                        }],
                        "database_specific": {"source_url": "https://x"}
                    },
                    {
                        "id": "GHSA-1111-0000-aaaa",
                        "summary": "A thing",
                        "aliases": []
                    }
                ],
                "next_page_token": "eyJfaWQ"
            })))
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn osv_query_canonicalizes() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let server = fixture_server().await;
        let env = capture_with_base(
            &server.uri(),
            OsvArgs {
                ecosystem: "PyPI",
                package: "requests",
                version: "2.30.0",
            },
        )
        .await
        .unwrap();
        let body = env.to_canonical_json().unwrap();
        // page token is stripped
        assert!(!body.contains("next_page_token"));
        // GHSA-1111 (alphabetically) before GHSA-9wx4
        assert!(body.find("GHSA-1111").unwrap() < body.find("GHSA-9wx4").unwrap());
        // alias and reference urls sorted
        assert!(body.find("CVE-2024-35195").unwrap() < body.find("PYSEC-2024-1").unwrap());
        assert!(body.find("a.example").unwrap() < body.find("b.example").unwrap());
        // database_specific.source_url stripped
        assert!(!body.contains("source_url"));
    }

    #[tokio::test]
    async fn osv_query_byte_identical() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let server = fixture_server().await;
        let a = capture_with_base(
            &server.uri(),
            OsvArgs {
                ecosystem: "PyPI",
                package: "requests",
                version: "2.30.0",
            },
        )
        .await
        .unwrap()
        .to_canonical_json()
        .unwrap();
        let b = capture_with_base(
            &server.uri(),
            OsvArgs {
                ecosystem: "PyPI",
                package: "requests",
                version: "2.30.0",
            },
        )
        .await
        .unwrap()
        .to_canonical_json()
        .unwrap();
        assert_eq!(a, b);
    }
}
