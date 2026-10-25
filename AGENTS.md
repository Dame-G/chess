# Driving the harness

You work through the `chess` CLI, which talks to a running runner (`chess serve`). Every command takes `--json` and prints one JSON document to stdout; errors go to stderr with a non-zero exit code. Every command maps onto one HTTP route, listed in `runner/openapi.json`.

```bash
chess status --json            # machine, runner version, Reckless checkout
chess token                    # the token the hosted UI (or a remote client) needs

chess build <ref> --wait       # build a branch, tag or commit, bench it, print the build
chess builds                   # every build: commit, bench (and whether it matches the commit's Bench line), nps
chess jobs | job <id> | log <id> | wait <id> | cancel <id>

chess analyse --fen "<fen>" --depth 20 --multipv 3 [--build <id>] [--moves e2e4 e7e5]
```

Builds and other heavy work are **jobs**: queued, run one at a time, each with a log. `chess build` prints the queued job; `--wait` (or `chess wait <id>`) blocks until it finishes and exits non-zero if it failed or was cancelled. A commit that is already built is not rebuilt.

A build's bench must equal the commit's `Bench:` line. When it differs, the build is suspect (non-determinism, or the commit message is stale): say so rather than testing with it.

## Experiments

An experiment asks one question: is `--dev` better than (or no worse than) `--base`? It runs a funnel of stages, and the first stage that fails stops it with its evidence.

```bash
chess experiment new --dev <branch> [--base main] --name "<short name>" --hypothesis "<what and why>" \
  [--kind gainer|nonregression|nonfunctional] [--ltc] [--wait]
chess experiment list [--batch <id>] | show <id> | wait <id> | cancel <id>

chess batch submit <file.json | -> --name "<name>" [--wait]   # a JSON array of experiments, as `experiment new` takes them
chess batch show <id> | wait <id>                              # one results table for the lot

chess test --dev <ref> [--base main] --preset stc [--bounds 0,3 | --pairs 500] [--wait]   # one match, no funnel
```

The funnels mirror Reckless's own OpenBench settings:

- **gainer:** bench must change, then a screen at VSTC 4+0.04 with SPRT [-5, 0] (rejects clear losses in minutes), then STC 8+0.08 [0, 3], then LTC 40+0.4 [0, 3] with `--ltc`.
- **nonregression** (simplifications, refactors that change search): the same, with STC at [-2.75, 0.25].
- **nonfunctional:** bench must be identical, then a paired speed comparison. No games.

Bounds are normalized Elo; the test is a pentanomial GSPRT with alpha 0.05 and beta 0.10. Presets for `chess test`: vstc, stc, ltc, stc-hash, ltc-hash (2 MB / 8 MB hash pressure), smp (6 threads) and nodes (25k per move). Games are in each test's PGN, with eval, depth, nodes and time per move: `GET /api/tests/<id>/games/<index>`.

How to work with them:

- Commit the change on a branch in the fork with an up-to-date `Bench:` line, then queue the experiment. Several independent ideas go in one batch; they run one after another.
- Pick the kind honestly. A change meant to be neutral (a simplification) is nonregression; only a change that cannot alter search is nonfunctional, and the bench stage checks it.
- An accepted STC result is evidence, not proof: a gain under about 2 Elo needs LTC before it is merged.
- A rejected screen means the idea, as written, loses. Read a few games and the nElo estimate before trying a variant.

Live progress is a server-sent event stream: `curl -N http://127.0.0.1:7878/api/events?since=<last id>`. Each event has an `id`, a `kind` and JSON `data`; pass the last id you saw to resume without gaps.

The engine is the Reckless fork in `RECKLESS_DIR` (default `../reckless`). Experiments are branches there. Every engine commit message ends with `Bench: <nodes>`, the node count of `reckless bench`: it is the engine's functional fingerprint, and CI checks it.

## Working on the harness itself

- `runner/` is Rust (axum, rusqlite, utoipa). After changing a route or a type it returns, run `corepack pnpm api` and commit the regenerated `runner/openapi.json` and `web/src/api/schema.d.ts`.
- `web/` is Vue with [eos](https://eos.dame.gg) components, imported by their own paths (`@eos-ui/vue/components/EButton`), never the barrel. The theme ("Saddle": colours, fonts) is `web/src/theme.ts`, built by eos's `defineTheme`. Build every control from eos; when a pattern is missing, it belongs in the eos library, not in a hand-rolled element here.
- Before pushing: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `corepack pnpm lint`, `format:check`, `typecheck`, `knip`, `test`.
