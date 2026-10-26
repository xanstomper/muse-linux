# Contributing

1. Fork, branch (`feat/your-thing`), commit with clean messages.
2. `cargo test --lib` must pass (app + daemon), `cargo fmt` clean.
3. Match existing style: KISS, DRY, minimal deps — stdlib over crates when possible.
4. New daemon tools need: policy capability mapping + audit line + a live-matrix test in the verify script.
5. Never commit tokens/keys. Audit logs are redacted; keep them that way.
