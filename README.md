# akahu-mcp

An unofficial, read-only [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) server for accessing banking data through the [Akahu API](https://developers.akahu.nz/).

The server exposes account and transaction data to MCP clients over stdio. It intentionally implements no payment, transfer, authorization, connection-management, or other write operations.

## Features

- Read-only by construction: only three MCP tools are registered.
- Personally identifying information (PII) is masked by default.
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
| `AKAHU_MASK_PII` | No | PII masking. Defaults to enabled. Set to `false`, `0`, `no`, or `off` to return unmasked fields. |

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

PII masking is enabled unless the operator explicitly disables it with `AKAHU_MASK_PII=false` (or `0`, `no`, or `off`).

By default, the server redacts common personal fields such as:

- account holder names and formatted/account numbers;
- email addresses and phone numbers;
- postal/street addresses;
- personal names nested under account, counterparty, payer/payee, beneficiary, sender/recipient, user, owner, or contact objects.

Institution and merchant names are preserved. Transaction descriptions are also preserved because they are core transaction data and cannot be reliably classified as PII without destroying useful information. Treat all MCP output as sensitive even when masking is enabled.

Every tool response includes `pii_masked` so a client can tell whether masking was active.

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

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE)); or
- MIT License ([LICENSE-MIT](LICENSE-MIT)).

at your option.
