# Improving Dameless

How to make the engine stronger with the app: ideas, their funnels, reading verdicts, tuning, nets and promoting. Tool names are MCP tools; the CLI command beside each does the same.

## Before the first idea

1. `harness_status` (`dameless harness`): `trusted: true` means this machine's testing rejected an identical engine with the current CPU, fastchess and book. When it is false or out of date, run `check_harness` (`dameless harness check --pairs 2000 --wait`) and wait for its idea. It is kept when the SPRT rejected the identical branch, its Elo interval holds 0 and the 2,000-pair symmetry run is symmetric (LL ≈ WW, LD ≈ WD). A dropped check means no verdict on this machine can be trusted: stop and tell the user.
2. `get_dameless` and `list_ideas` (`dameless ideas --status dropped`): what has been tried, by whom and why it ended. A dropped idea is evidence; a variant of it needs a new reason.

## One idea, step by step

1. **Hypothesis.** One sentence of what changes and why it should gain: "Reduce LMR less for moves that give check, since checks are often forcing and the reduction misses tactics." Pick the **kind** honestly:
   - `gain`: meant to be stronger.
   - `neutral`: a simplification or refactor that changes search but should not lose.
   - `speed`: cannot change search (same bench), only faster.
   - `tune`: SPSA over the branch's tunables (see Tuning).
   - `net`: a trained network (see Nets).
2. **Branch.** In the fork (`RECKLESS_DIR`, usually `../reckless`), branch from the champion's commit (the fork's `main`), make the change, and commit. The commit message ends with `Bench: <nodes>`, the node count `reckless bench` prints for that commit: it is the engine's functional fingerprint. Not sure of the number? Commit with your best value and let the app fix it: `check_branch` says `ok`, `missing`, `stale` or `unbuilt`, and `fix_bench` (or `fix_bench: true` on the idea) builds the tip and rewords it with the measured bench, locally. A branch with a missing or stale line is refused with 409.
3. **Queue.** One idea with `create_idea`, several with `create_batch` (`dameless idea batch ideas.json --tag <tag>`). The tag names the batch; each idea runs its funnel, one trial at a time, in queue order:

   ```json
   {
     "tag": "lmr-checks",
     "ideas": [
       {
         "title": "Less LMR for checks",
         "kind": "gain",
         "branch": "claude/lmr-checks",
         "hypothesis": "Checks are forcing; reducing them less finds tactics LMR misses.",
         "notes": "Reduction 1 ply less when the move gives check. Upstream tried 2 plies in #812 and lost.",
         "author": "claude"
       }
     ]
   }
   ```

   `notes` is markdown: your reasoning, what you looked at, what would change your mind. `long: true` adds the LTC stage. `base` defaults to the champion.

4. **Wait.** `wait_for_idea` (answers `{ finished, idea }`; call again until `finished`), or `dameless idea show <id>`. Trials run as background work: `list_work`, `get_work`, `work_log` show where they are. Work pauses for the user's live games and resumes by itself.
5. **Report.** The verdict sentence, each trial's Elo ± and LLR, and what it means (next section). Then the next idea, or the user's decision on promoting.

## The funnels

These copy Reckless's OpenBench. Bounds are normalized Elo; each game stage is a pentanomial GSPRT with alpha 0.05 and beta 0.10. The first stage that fails ends the idea as dropped, with its evidence.

| Kind    | Stages                                                                                                                        |
| ------- | ----------------------------------------------------------------------------------------------------------------------------- |
| gain    | fingerprint (bench must change) → screen VSTC 4+0.04 [-5, 0] → short STC 8+0.08 [0, 3] → long LTC 40+0.4 [0, 3] (with `long`) |
| neutral | fingerprint (bench must change) → screen VSTC [-5, 0] → short STC [-2.75, 0.25] → long LTC [-2.75, 0.25] (with `long`)        |
| speed   | fingerprint (bench must be identical) → speed (5 paired bench runs; dropped when measurably slower)                           |
| tune    | tune (SPSA writes the change) → fingerprint → screen → confirm STC [0, 4] → long LTC [0, 4] (with `long`)                     |
| net     | train (a run makes the net) → fingerprint → screen → short STC [0, 3] → long LTC [0, 3] (with `long`)                         |

The fingerprint also fails when either build's bench differs from its commit's `Bench:` line: a stale line, or search that is not deterministic. Say which, rather than retrying.

A **custom** idea is one trial at any preset with bounds or a fixed number of pairs: `create_idea` with `custom: { preset, bounds | pairs }`, or `dameless test --dev <branch> --preset stc --bounds 0,3`. Presets: `vstc` 4+0.04, `stc` 8+0.08, `ltc` 40+0.4 (64 MB), `stc_hash` (2 MB hash), `ltc_hash` (8 MB), `smp` (40+0.4, 6 threads, 512 MB) and `nodes` (25,000 nodes a move). Use `stc_hash` for transposition-table changes and `smp` for parallel search.

**Clocks are scaled to this machine**, as OpenBench scales them: before each trial the app benches base and dev with as many copies as the trial's threads and multiplies the clock by 660,000 / base's nps. A trial's `played` and `scaling` record it ("8+0.08" may be played as 11+0.11). Node limits are never scaled.

## Reading a verdict

- **A passed screen is not evidence of a gain.** At [-5, 0] a change that does nothing passes about 90 % of the time; the screen only throws out clear losses. The short (STC) trial decides.
- **Kept at STC is evidence, not proof.** A gain under about 2 Elo needs the long (LTC) trial before anyone relies on it; queue the idea again with `long: true` or a custom LTC trial.
- **Dropped at the screen** means the idea, as written, loses. Look at a few of its games (`list_games` with `trial`, then `get_game_pgn`) and the nElo estimate before trying a variant, and say what the variant changes.
- **Report numbers as they are**: Elo ± its interval, LLR against its bounds, pairs played. A trial that ran out of pairs or was stopped decided nothing.
- While `harness_status.trusted` is false, every verdict carries that warning: repeat it.

## Promoting and pushing

`promote_idea` merges a kept idea's branch into the fork's `main` locally (fast-forward, or a merge commit carrying the `Bench:` line) and makes it the next champion `v<n+1>`. It is refused (409, nothing changed) when the user's checkout of `main` has uncommitted changes. Promote when the user wants it; after a promotion, new ideas branch from the new `main`.

`push` sends the fork's `main` to `origin` (GitHub). It runs only when the user asks for that push.

## Tuning

A tune moves search constants by SPSA, as OpenBench does. The tunables are declared in the fork's `src/parameters.rs` with `define!` (`i32 name: default, min, max, c_end;`); the branch `spsa/tunables` declares the current set. Without the `spsa` cargo feature each is a `const fn`, so the bench is unchanged.

- `list_tunables` (`dameless tunables spsa/tunables`) lists what a branch declares; it answers 409 while the branch's `spsa` build is still being made, and queues it.
- Queue with `create_idea` (`kind: "tune"`, `branch`, optional `tune: { params, iterations, pairs, time, hash }`) or `dameless tune --branch spsa/tunables --params a,b --iterations 2500`. Defaults are OpenBench's: 1,000 iterations of 8 pairs at STC, alpha 0.602, gamma 0.101, A = 0.1 × iterations, r_end 0.002.
- The schedule is fixed by the iteration count. 1,000 × 8 pairs (16,000 games) shows a tune works; for many parameters use 5,000 iterations or more, or tune fewer parameters (Reckless's own sessions run 95,000 to 200,000 games).
- When SPSA ends it writes the rounded values on `tune/<idea id>` with its `Bench:` line, and the confirm trial at [0, 4] decides. Trust the confirmation, not the trajectory (`get_tune`).

## Nets

A trained network is an idea of kind `net`: a training run fine-tunes the base's net (or trains from scratch), the version is the champion's code plus the new net, and the usual trials follow. It needs a GPU (409 without one). Datasets come from `datagen` (self-play), `import_dataset` (a download the user confirms with its size) or `dataset_from_trials`. Datasets share a disk cap (`list_datasets` shows it and the room left), and nothing that writes much starts with less than 15 GB free. Ask the user before any download or a datagen run of more than a few GB.

## Versions

A version is a commit that matters: the champion line (`v1`, `v2`, …), an idea's branch, an upstream tip. `list_versions`, `make_version` (`{ "ref": "<branch, tag or commit>" }`, which queues its build) and `list_builds` cover them. Requests name versions as `champion`, `v14`, `#12` (an id), or a branch, tag or commit.
