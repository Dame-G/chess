# Dameless (rebuild in progress)

Read `docs/design.md` first: it is the contract for every crate, route, event, table and screen. The old app (branch `main`, `C:\dev\chess`) is the reference for the parts being ported.

## Running the app

```bash
cargo run -p dameless -- serve                 # http://127.0.0.1:7979 (DAMELESS_LISTEN), data in DAMELESS_DATA
cargo run -p dameless -- status --json         # any other subcommand is a client of the running app
```

- `dameless serve` reads `DAMELESS_DATA` (from the repo: `DAMELESS_DATA=.data`, which is ignored), `RECKLESS_DIR` (default `../reckless`), `DAMELESS_CORES` and `LICHESS_URL`. Never point a hand-run app at the real Lichess: set `LICHESS_URL` to a mock or an unreachable address.
- The CLI finds the app at `DAMELESS_URL` (default http://127.0.0.1:7979) with `DAMELESS_TOKEN` or the token in the data directory. Every command takes `--json`; the full list is in `docs/design.md` → API, and `dameless --help`.
- After changing a route or a type it returns, run `corepack pnpm api` and commit `crates/server/openapi.json` and `apps/web/src/api/schema.d.ts`.
