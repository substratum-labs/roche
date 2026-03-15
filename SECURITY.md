# Security Policy

## About Roche and Security

Roche is a sandbox orchestrator designed specifically for executing untrusted code on behalf of AI agents. Security is foundational to the project, not an afterthought. We take all vulnerability reports seriously.

## Supported Versions

| Version | Supported |
| ------- | --------- |
| 0.1.x   | Yes       |
| < 0.1   | No        |

## Reporting a Vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Instead, email **security@substratum-labs.com** with:

- A description of the vulnerability.
- Steps to reproduce or a proof of concept.
- The affected version(s).
- Any potential impact assessment.

## What to Expect

- **Acknowledgment** within 48 hours of your report.
- We will work with you to understand and validate the issue.
- A fix will be developed privately and released as a patch.
- We follow a **90-day disclosure timeline** from the initial report. If a fix is released sooner, we will coordinate public disclosure with you at that time.

## Scope

The following are in scope for security reports:

- Sandbox escapes (container, VM, or WASM boundary violations)
- Privilege escalation within or from a sandbox
- Unauthorized network access from sandboxed environments
- Vulnerabilities in the daemon or gRPC interface
- Dependency vulnerabilities with a demonstrated exploit path

## Recognition

We appreciate responsible disclosure and are happy to credit reporters in release notes upon request.
