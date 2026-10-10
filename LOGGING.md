# Logging

Structured logging with `tracing`. Control verbosity with `RUST_LOG`:

```bash
RUST_LOG=debug just run   # everything
RUST_LOG=warn just run    # warnings and errors only
just run                  # default level: info
```

Rules:

- Never log secrets: no passwords, keys, tokens, vault contents, or
  decrypted material — not even at debug level.
- Log facts (state transitions, IO results, timings), not QML chatter:
  the UI layer reports user-facing errors, it does not trace them here.
- Prefer spans around operations (`unlock`, `persist`) over scattered
  one-line events when following a flow matters.
