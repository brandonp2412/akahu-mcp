# Security Policy

## Supported versions

Security fixes are applied to the latest release and the default branch.

## Reporting a vulnerability

Please use GitHub's private vulnerability reporting for this repository rather than opening a public issue for undisclosed security problems.

Do not include real Akahu access tokens, app ID tokens, account numbers, transaction records, or other banking data in reports, logs, screenshots, or test fixtures.

## Scope

Security-sensitive areas include:

- credential handling;
- accidental logging or exposure of API responses;
- PII masking bypasses;
- URL/path construction;
- MCP tool registration that could introduce write capabilities;
- dependency vulnerabilities that affect the server at runtime.

The server is intentionally read-only. A change that introduces payment, transfer, authorization, connection-management, or other Akahu mutation functionality should be treated as a security-sensitive design change.
