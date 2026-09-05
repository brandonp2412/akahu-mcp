# Contributing

Contributions are welcome.

## Development

Use a current stable Rust toolchain compatible with the `rust-version` declared in `Cargo.toml`.

Before submitting a change, run:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-features --locked
cargo audit
cargo package --locked
```

## Security and privacy

Never commit real Akahu credentials, account numbers, names, transaction records, or other banking data. Tests and examples must use synthetic data.

PII masking is a default-on safety feature. Changes that reduce its coverage should include explicit justification and tests.

The project is intentionally read-only. Do not add payment, transfer, authorization, connection-management, or other mutation tools without first opening a design discussion.

## Pull requests

Keep changes focused and include tests for behavior changes. Update the README when configuration, tool schemas, privacy behavior, or operational requirements change.
