# Dameless

A development harness for a fork of the [Reckless](https://github.com/codedeliveryservice/Reckless) chess engine. It builds engine variants, runs them through a testing funnel, analyses and plays games, and (later) tunes parameters and trains networks. People use it through the web UI; agents use the same capabilities through the `chess` CLI and the HTTP API.

The UI is published at https://chess.dame.gg. All data stays with a **runner**, a local program (`chess serve`) on the machine that does the work. The hosted UI is a convenience: the runner serves the same UI itself.

| Path        | What                                                                                   |
| ----------- | -------------------------------------------------------------------------------------- |
| `runner/`   | The `chess` binary (Rust): API, live events, SQLite store, CLI, embedded UI            |
| `web/`      | The UI (Vue, [eos](https://eos.dame.gg)), built into `web/dist`                        |
| `scripts/`  | `api-types.mjs`: regenerates the web client's types from the runner's OpenAPI document |
| `AGENTS.md` | How an agent drives the harness                                                        |

## Run it

The Reckless checkout sits beside this repo (`../reckless`). Set `RECKLESS_DIR` to use one elsewhere.

```bash
corepack pnpm install
corepack pnpm build            # the UI, into web/dist
cargo run -p chess -- serve    # http://127.0.0.1:7878
```

For UI work, run the runner and `corepack pnpm dev` (http://localhost:5173); the dev server proxies `/api` to the runner.

### Using the hosted UI with a local runner

chess.dame.gg can reach a runner on your machine. Run `chess token`, paste the token into the Runner page, and allow the browser's prompt for access to devices on your local network. The runner accepts the hosted origin only with that token, checks the `Host` of every request, and listens on `127.0.0.1` unless told otherwise. A runner on another machine (`--listen 0.0.0.0:7878`, behind Tailscale or a Cloudflare Tunnel) requires the token on every request.

## Checks

```bash
cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test
corepack pnpm lint && corepack pnpm format:check && corepack pnpm typecheck && corepack pnpm knip && corepack pnpm test
corepack pnpm api              # after changing the API: regenerate runner/openapi.json and web/src/api/schema.d.ts
```

CI runs all of these; `ci-ok` is the one required check. A green run on `main` deploys the UI to Cloudflare once the `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` secrets are set.

## License

[GPL-3.0-or-later](LICENSE). The board (chessground) and chess logic (chessops, shakmaty) are GPL, and Reckless is AGPL.
