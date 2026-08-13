//! Source-side dependency-audit and SBOM capturers.
//!
//! Five actions, one canonical envelope shape each. Library APIs where
//! practical (`rustsec` for cargo, REST for OSV); subprocess fallback
//! for pip, pnpm, and the three CycloneDX generators where no
//! Rust-callable library exists.
//!
//! The cargo-audit capturer runs `rustsec` without its `git`/`gix`
//! features (those pull a vulnerable `gix` tree): the advisory db is
//! either supplied via `--db-path` / `SECUNIT_RUSTSEC_DB` or downloaded
//! over plain HTTPS as a tarball — never via the `gix` git client.

pub mod cargo_audit;
pub mod cmd;
pub mod osv_query;
pub mod pip_audit;
pub mod pnpm_audit;
pub mod sbom;

/// Shorten `s` to at most `n` bytes for quoting in an error message.
///
/// Truncation walks back to the nearest UTF-8 character boundary.
/// Slicing `&s[..n]` directly panics when `n` lands mid-sequence, and
/// every caller here quotes subprocess output — cargo diagnostics use
/// `─` and curly quotes, npm uses `‼`, so a boundary hit is routine.
/// A panic would replace the intended exit-2 error (and the
/// `.pending.txt` remediation a sweep writes from it) with a backtrace.
pub(crate) fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        return s.to_string();
    }
    let mut end = n;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &s[..end])
}

#[cfg(test)]
mod tests {
    use super::truncate;

    #[test]
    fn shorter_than_the_limit_is_returned_whole() {
        assert_eq!(truncate("abc", 10), "abc");
        assert_eq!(truncate("abc", 3), "abc");
    }

    #[test]
    fn ascii_truncates_at_the_limit() {
        assert_eq!(truncate("abcdef", 3), "abc...");
    }

    #[test]
    fn a_limit_inside_a_multibyte_sequence_does_not_panic() {
        // `─` is 3 bytes (U+2500). Cutting at 1 or 2 bytes into it must
        // walk back to the boundary rather than panic.
        let s = "ab─cd";
        for n in 0..s.len() {
            let out = truncate(s, n);
            assert!(s.starts_with(out.trim_end_matches('.')));
        }
        assert_eq!(truncate(s, 3), "ab...");
        assert_eq!(truncate(s, 4), "ab...");
        assert_eq!(truncate(s, 5), "ab─...");
    }

    #[test]
    fn a_limit_of_zero_yields_only_the_ellipsis() {
        assert_eq!(truncate("abc", 0), "...");
    }
}
