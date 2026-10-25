# Driving the harness

You work through the `chess` CLI, which talks to a running runner (`chess serve`). Every command takes `--json` and prints one JSON document to stdout; errors go to stderr with a non-zero exit code. Every command maps onto one HTTP route, listed in `runner/openapi.json`.

```bash
chess status --json            # machine, runner version, Reckless checkout
chess token                    # the token the hosted UI (or a remote client) needs
```

Live progress is a server-sent event stream: `curl -N http://127.0.0.1:7878/api/events?since=<last id>`. Each event has an `id`, a `kind` and JSON `data`; pass the last id you saw to resume without gaps.

The engine is the Reckless fork in `RECKLESS_DIR` (default `../reckless`). Experiments are branches there. Every engine commit message ends with `Bench: <nodes>`, the node count of `reckless bench`: it is the engine's functional fingerprint, and CI checks it.

## Working on the harness itself

- `runner/` is Rust (axum, rusqlite, utoipa). After changing a route or a type it returns, run `corepack pnpm api` and commit the regenerated `runner/openapi.json` and `web/src/api/schema.d.ts`.
- `web/` is Vue with [eos](https://eos.dame.gg) components, imported by their own paths (`@eos-ui/vue/components/EButton`), never the barrel. Build every control from eos; when a pattern is missing, it belongs in the eos library, not in a hand-rolled element here.
- Before pushing: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `corepack pnpm lint`, `format:check`, `typecheck`, `knip`, `test`.
