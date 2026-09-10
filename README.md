<p align="center">
  <img src="assets/logo.svg" alt="akahu-mcp" width="900">
</p>

An unofficial, read-only [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) server for accessing banking data through the [Akahu API](https://developers.akahu.nz/).

The server exposes account and transaction data to MCP clients over stdio. It intentionally implements no payment, transfer, authorization, connection-management, or other write operations.

## Features

- Read-only by construction: only three MCP tools are registered.
- Privacy masking is enabled by default without removing the financial context an MCP client needs to answer useful questions.
- Settled and pending transactions are kept separate.
- Settled transaction queries default to the most recent 30 days.
- Cursor pagination with a safety cap and optional result limits.
- Retry/backoff for transient network errors, rate limits, and Akahu 5xx responses.
- Rustls-based HTTPS with no OpenSSL runtime dependency.
- MCP read-only annotations on every tool.

## Tools

| Tool | Purpose |
| --- | --- |
| `list_accounts` | Lists connected accounts, balances, types, status, and attributes. |
| `get_transactions` | Gets settled transactions, optionally scoped by account, date range, and result limit. |
| `get_pending_transactions` | Gets pending transactions, optionally scoped by account and result limit. |

`get_transactions` uses UTC ISO 8601 timestamps. `start` is exclusive and `end` is inclusive. If no range is supplied, the server queries the previous 30 days.

## Requirements

- Rust 1.88 or newer to build from source.
- Akahu API credentials with access to the data you want to expose.

Set the following environment variables:

| Variable | Required | Description |
| --- | --- | --- |
| `AKAHU_ACCESS_TOKEN` | Yes | Akahu user access token. |
| `AKAHU_APP_ID_TOKEN` | Yes | Akahu app ID token. |
| `AKAHU_MASK_PII` | No | Privacy masking. Defaults to enabled. Set to `false`, `0`, `no`, or `off` to return completely unmasked Akahu fields. |

Never commit real Akahu credentials to a repository or MCP client configuration that is shared with others.

## Build

```sh
cargo build --release --locked
```

The resulting binary is `target/release/akahu-mcp`.

To verify credentials without printing banking data:

```sh
AKAHU_ACCESS_TOKEN=... AKAHU_APP_ID_TOKEN=... ./target/release/akahu-mcp --health-check
```

## MCP client configuration

Configure your MCP client to start `akahu-mcp` over stdio and provide the required environment variables. For example, after installing the binary somewhere on `PATH`, use `akahu-mcp` as the MCP server command.

The server supports conventional MCP initialization and also accepts direct tool requests, which is useful for transports that manage the MCP lifecycle externally.

## PII masking

Privacy masking is enabled unless the operator explicitly disables it with `AKAHU_MASK_PII=false` (or `0`, `no`, or `off`). The default policy is deliberately usefulness-preserving: it does not hide the people, counterparties, merchants, transactions, balances, descriptions, account IDs, or other financial context needed to reason about the data.

By default, the server:

- fully redacts credentials, access/refresh tokens, API keys, passwords, client secrets, and other reusable secret fields if they ever appear in an Akahu response; Akahu's `_authorisation` object ID is preserved because it is not login credentials;
- fully redacts contact details such as email addresses, phone numbers, and postal/street addresses;
- partially masks full bank-account/IBAN and payment-card numbers while preserving the final four alphanumeric characters so accounts remain distinguishable;
- preserves account-holder names, counterparty/payee/payer names, Akahu object IDs, transaction hashes, card suffixes, institution names, merchant names, descriptions, amounts, balances, dates, categories, and other transaction metadata.

This masking is a guard against unnecessarily exposing secrets or directly reusable identifiers, not an anonymization layer. The MCP output still contains sensitive financial data and should be treated accordingly.

Every tool response includes `pii_masked` so a client can tell whether the privacy policy was active.

## Security model

This project deliberately contains no write tools. MCP read-only annotations are advisory metadata; the stronger guarantee is that there is no code path implementing Akahu payments or mutations.

The API base URL is fixed to Akahu's HTTPS endpoint. Caller-supplied account IDs are added as URL path segments through the URL library rather than string interpolation. HTTP response bodies are not logged.

See [SECURITY.md](SECURITY.md) for reporting vulnerabilities.

## Akahu usage

This project is not affiliated with or endorsed by Akahu. You are responsible for complying with Akahu's current developer terms, API permissions, and any restrictions that apply to Personal Apps or production integrations.

Akahu's current Personal Apps documentation describes Personal Apps as a one-user service restricted to your own Akahu account, with account-information access but no payments or webhooks. For multi-user or public/production integrations, Akahu documents a full-app and accreditation process. Publishing or self-hosting this MCP server does not change those platform requirements.

- [Personal Apps](https://developers.akahu.nz/docs/personal-apps)
- [App Accreditation](https://developers.akahu.nz/docs/app-accreditation)

## Development

```sh
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-features --locked
cargo audit
cargo package --locked
```

## License

Licensed under the MIT License. See [LICENSE](LICENSE).
