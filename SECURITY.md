# Security Policy

`secunit` is the operational layer for Battle Creek's Written Information
Security Program — it allocates evidence run directories, hashes artifacts,
assembles tamper-evident manifests, and captures evidence from upstream
integrations. Because it handles audit evidence and reads credentials for
those integrations, we take reports about it seriously.

## Reporting a Vulnerability

Please report security vulnerabilities privately via GitHub's
[private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability)
feature on this repository's **Security** tab.

We aim to acknowledge reports within 5 business days and provide an
initial assessment within 10 business days. Please do not file public
issues for security-sensitive reports.

When reporting, please include where relevant:

- The affected `secunit` version (`secunit --version`) and cargo features
  compiled in (`secunit features`).
- A description of the issue and its impact — especially anything that
  could undermine evidence integrity (manifest hash chaining, `verify`),
  leak integration credentials, or write outside an allocated run dir.
- Reproduction steps or a proof of concept.

## Scope

In scope: the `secunit` binary and its crates (`secunit-core`,
`bcl-secunit`/CLI, `secunit-capture`, the Tauri GUI), the evidence
hash-chain and verification logic, and the native capturers that read
integration credentials.

Out of scope: vulnerabilities in third-party services that capturers query
(AWS, GitHub, OSV) — report those to the respective vendor. The contents of
any organization's private WISP registry are also out of scope here.

## Handling of Secrets

`secunit` capturers read credentials from the standard credential chain or
the `_config.yaml` integration block and never persist them across
invocations. Please do not include live tokens, keys, or other secrets in
vulnerability reports; redact them.

## Supported Versions

This project follows a rolling-release model — only the current `main`
branch and the latest tagged release are supported. There is no backport
of fixes to older `0.1.x` tags.
