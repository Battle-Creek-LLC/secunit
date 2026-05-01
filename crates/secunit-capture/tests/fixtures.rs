//! Fixture-driven integration tests for every capturer.
//!
//! These tests prove the Phase 4 contract:
//!   1. Two consecutive runs of the same fixture produce byte-identical
//!      canonical output.
//!   2. The canonical output validates against the published schema.
//!
//! Fixtures are parked under `testdata/fixtures/captures/<subsystem>/<action>/`.

use std::fs;
use std::path::{Path, PathBuf};

use secunit_capture::canonical::Envelope;
use secunit_capture::time::set_fixed_time_for_tests;
use serde_json::{json, Value};

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("testdata/fixtures/captures")
        .canonicalize()
        .expect("locate testdata/fixtures/captures")
}

fn read_json(p: &Path) -> Value {
    let text = fs::read_to_string(p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", p.display()))
}

fn assert_byte_identical_and_schema_valid(env_a: &Envelope, env_b: &Envelope) {
    let a = env_a.to_canonical_json().unwrap();
    let b = env_b.to_canonical_json().unwrap();
    assert_eq!(
        a, b,
        "two captures of the same fixture must be byte-identical"
    );
    let errs = secunit_capture::schema::validate(env_a).expect("schema lookup");
    assert!(errs.is_empty(), "schema mismatch: {errs:?}");
}

// -----------------------------------------------------------------------------
// deps
// -----------------------------------------------------------------------------

#[cfg(feature = "deps")]
mod deps_fixtures {
    use super::*;
    use secunit_capture::deps::cmd::testing::CannedRunner;
    use secunit_capture::deps::cmd::CmdOutput;

    fn pip_audit_runner(stdout: String) -> CannedRunner {
        let r = CannedRunner::new();
        r.register(
            "pip-audit",
            &["--format=json", "--strict", "-r", "requirements.txt"],
            CmdOutput {
                stdout,
                stderr: String::new(),
                exit_code: 1,
            },
        );
        r
    }

    /// Stage a scratch project dir containing a `requirements.txt` so the
    /// capturer's manifest detection picks it up.
    fn pip_audit_project_dir() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("requirements.txt"), "# placeholder\n").unwrap();
        d
    }

    fn pnpm_runner(stdout: String) -> CannedRunner {
        let r = CannedRunner::new();
        r.register(
            "pnpm",
            &["audit", "--json"],
            CmdOutput {
                stdout,
                stderr: String::new(),
                exit_code: 1,
            },
        );
        r
    }

    #[test]
    fn pip_audit_fixture_roundtrips() {
        let _g = set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let fx = fixture_root().join("deps/pip-audit/sample.json");
        let stdout = fs::read_to_string(&fx).unwrap();
        let runner = pip_audit_runner(stdout);
        let d = pip_audit_project_dir();
        let a = secunit_capture::deps::pip_audit::capture_with(d.path(), &runner).unwrap();
        let b = secunit_capture::deps::pip_audit::capture_with(d.path(), &runner).unwrap();
        assert_byte_identical_and_schema_valid(&a, &b);
    }

    #[test]
    fn pnpm_audit_fixture_roundtrips() {
        let _g = set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let fx = fixture_root().join("deps/pnpm-audit/sample.json");
        let stdout = fs::read_to_string(&fx).unwrap();
        let runner = pnpm_runner(stdout);
        let a =
            secunit_capture::deps::pnpm_audit::capture_with(Path::new("/tmp"), &runner).unwrap();
        let b =
            secunit_capture::deps::pnpm_audit::capture_with(Path::new("/tmp"), &runner).unwrap();
        assert_byte_identical_and_schema_valid(&a, &b);
    }

    /// cargo-audit's library entry point requires a real advisory db
    /// clone, so we exercise the canonicalization seam directly:
    /// load a recorded `rustsec::Report`-shaped fixture, canonicalize
    /// it, wrap in an envelope, and check the envelope round-trips
    /// byte-identically and validates against the schema.
    #[test]
    fn cargo_audit_fixture_roundtrips() {
        let _g = set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let fx = fixture_root().join("deps/cargo-audit/synthetic-report.json");
        let mut raw_a = read_json(&fx);
        let mut raw_b = read_json(&fx);
        let result_a = secunit_capture::deps::cargo_audit::canonicalize_report(&mut raw_a);
        let result_b = secunit_capture::deps::cargo_audit::canonicalize_report(&mut raw_b);
        let args = json!({"lockfile": "/x", "db_path": null});
        let env_a = Envelope::new("deps.cargo-audit", "1", args.clone(), result_a);
        let env_b = Envelope::new("deps.cargo-audit", "1", args, result_b);
        assert_byte_identical_and_schema_valid(&env_a, &env_b);
    }

    #[tokio::test]
    async fn osv_query_fixture_roundtrips() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let _g = set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let fx = fixture_root().join("deps/osv-query/requests-2.30.json");
        let body = read_json(&fx);

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/query"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let args = secunit_capture::deps::osv_query::OsvArgs {
            ecosystem: "PyPI",
            package: "requests",
            version: "2.30.0",
        };
        let a = secunit_capture::deps::osv_query::capture_with_base(&server.uri(), args)
            .await
            .unwrap();
        let args2 = secunit_capture::deps::osv_query::OsvArgs {
            ecosystem: "PyPI",
            package: "requests",
            version: "2.30.0",
        };
        let b = secunit_capture::deps::osv_query::capture_with_base(&server.uri(), args2)
            .await
            .unwrap();
        assert_byte_identical_and_schema_valid(&a, &b);
    }
}

// -----------------------------------------------------------------------------
// github
// -----------------------------------------------------------------------------

#[cfg(feature = "github")]
mod github_fixtures {
    use super::*;
    use secunit_capture::github::GhClient;
    use wiremock::matchers::{method, path, query_param, query_param_is_missing};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn empty_page_2(server: &MockServer, route: &str) {
        Mock::given(method("GET"))
            .and(path(route))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn dependabot_alerts_fixture_roundtrips() {
        let _g = set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let fx = fixture_root().join("github/dependabot-alerts/page-1.json");
        let body = read_json(&fx);
        let server = MockServer::start().await;
        // Dependabot alerts uses cursor pagination via the `Link` header,
        // not `?page=N`. The first response carries `rel="next"` pointing
        // at an empty follow-up page.
        let next_url = format!(
            "{}/repos/o/r/dependabot/alerts?per_page=100&after=CUR1",
            server.uri()
        );
        Mock::given(method("GET"))
            .and(path("/repos/o/r/dependabot/alerts"))
            .and(query_param_is_missing("after"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Link", format!(r#"<{next_url}>; rel="next""#))
                    .set_body_json(body),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/o/r/dependabot/alerts"))
            .and(query_param("after", "CUR1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;

        let c = GhClient::with_base_uri(&server.uri(), Some("ghp_x")).unwrap();
        let a = secunit_capture::github::dependabot_alerts::capture(&c, "o", "r", None)
            .await
            .unwrap();
        let b = secunit_capture::github::dependabot_alerts::capture(&c, "o", "r", None)
            .await
            .unwrap();
        assert_byte_identical_and_schema_valid(&a, &b);
    }

    #[tokio::test]
    async fn branch_protection_fixture_roundtrips() {
        let _g = set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let fx = fixture_root().join("github/branch-protection/main.json");
        let body = read_json(&fx);
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/o/r/branches/main/protection"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
        let c = GhClient::with_base_uri(&server.uri(), Some("ghp_x")).unwrap();
        let a = secunit_capture::github::branch_protection::capture(&c, "o", "r", "main")
            .await
            .unwrap();
        let b = secunit_capture::github::branch_protection::capture(&c, "o", "r", "main")
            .await
            .unwrap();
        assert_byte_identical_and_schema_valid(&a, &b);
    }

    #[tokio::test]
    async fn org_members_fixture_roundtrips() {
        let _g = set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let fx = fixture_root().join("github/org-members/page-1.json");
        let body = read_json(&fx);
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/acme/members"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
        empty_page_2(&server, "/orgs/acme/members").await;
        let c = GhClient::with_base_uri(&server.uri(), Some("ghp_x")).unwrap();
        let a = secunit_capture::github::org_members::capture(&c, "acme")
            .await
            .unwrap();
        let b = secunit_capture::github::org_members::capture(&c, "acme")
            .await
            .unwrap();
        assert_byte_identical_and_schema_valid(&a, &b);
    }

    #[tokio::test]
    async fn audit_log_fixture_roundtrips() {
        let _g = set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let fx = fixture_root().join("github/audit-log/page-1.json");
        let body = read_json(&fx);
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/acme/audit-log"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
        empty_page_2(&server, "/orgs/acme/audit-log").await;
        let c = GhClient::with_base_uri(&server.uri(), Some("ghp_x")).unwrap();
        let a = secunit_capture::github::audit_log::capture(&c, "acme", "2026-04-01T00:00:00Z")
            .await
            .unwrap();
        let b = secunit_capture::github::audit_log::capture(&c, "acme", "2026-04-01T00:00:00Z")
            .await
            .unwrap();
        assert_byte_identical_and_schema_valid(&a, &b);
    }

    #[tokio::test]
    async fn codeql_alerts_fixture_roundtrips() {
        let _g = set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let fx = fixture_root().join("github/codeql-alerts/page-1.json");
        let body = read_json(&fx);
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/o/r/code-scanning/alerts"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
        empty_page_2(&server, "/repos/o/r/code-scanning/alerts").await;
        let c = GhClient::with_base_uri(&server.uri(), Some("ghp_x")).unwrap();
        let a = secunit_capture::github::codeql_alerts::capture(&c, "o", "r")
            .await
            .unwrap();
        let b = secunit_capture::github::codeql_alerts::capture(&c, "o", "r")
            .await
            .unwrap();
        assert_byte_identical_and_schema_valid(&a, &b);
    }
}
