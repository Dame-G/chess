import type { EntryKey } from "@pinia/colada";
import type { RunnerEvent } from "./events";
import type { Job, Test, TestDetail, Tune, TuneDetail, Version } from "./queries";

/** The part of Pinia Colada's query cache the events touch. */
export interface RunnerCache {
  getQueryData(key: EntryKey): unknown;
  setQueryData(key: EntryKey, data: unknown): void;
  /** With no filter, every query. */
  invalidateQueries(filters?: { key: EntryKey }): unknown;
}

/**
 * Keeps the query cache in step with the runner's live events: refetches what an event changed, or folds the event in
 * when it carries the new state itself. The app calls it for every event of its one stream.
 */
export function applyRunnerEvent(cache: RunnerCache, { kind, data }: RunnerEvent) {
  const refetch = (...keys: EntryKey[]) => keys.forEach((key) => cache.invalidateQueries({ key }));

  switch (kind) {
    // A (re)started runner, or a stream that reconnected, may have changed anything.
    case "runner.started":
      cache.invalidateQueries();
      break;
    case "job.updated": {
      refetch(["jobs"]);
      const { status, spec } = data as Pick<Job, "status"> & { spec?: { kind?: string } };
      // A finished job may have made a build, a branch (an applied tune) or a verdict.
      if (status === "done") refetch(["builds"], ["branches"], ["versions"]);
      // An import made matches.
      if (status === "done" && spec?.kind === "lichess_import") refetch(["matches"]);
      else if (status === "failed") refetch(["versions"]);
      break;
    }
    case "versions.changed":
      refetch(["versions"], ["branches"], ["builds"]);
      break;
    case "experiment.updated": {
      const { id } = data as { id: number };
      refetch(["experiments"], ["experiment", id], ["versions"]);
      break;
    }
    case "test.progress":
      applyTestProgress(cache, data as Test);
      break;
    case "tune.updated":
    case "tune.finished":
      applyTuneEvent(cache, kind, data);
      refetch(["versions"]);
      break;
    case "tune.progress": {
      applyTuneEvent(cache, kind, data);
      // A version shows its tune's iteration: patched in place, since a tune reports many times a minute.
      const p = data as { id: number; iteration: number };
      const versions = cache.getQueryData(["versions"]) as Version[] | undefined;
      if (versions?.some((v) => v.tune?.id === p.id))
        cache.setQueryData(
          ["versions"],
          versions.map((v) => (v.tune?.id === p.id ? { ...v, tune: { ...v.tune, iteration: p.iteration } } : v)),
        );
      break;
    }
    // Local and Lichess games share the `matches` prefix. Moves go to the board through `useRunnerEvent`.
    case "match.started":
    case "match.ended":
      refetch(["matches"]);
      break;
    case "lichess.status":
      cache.setQueryData(["lichess"], data);
      break;
    // Your own account: the status carries the challenges and the seek, so a challenge event only has to refetch it.
    case "lichess.me.status":
      cache.setQueryData(["lichess-me"], data);
      break;
    case "lichess.me.challenge":
      refetch(["lichess-me"]);
      break;
    case "lichess.me.fair_play": {
      const me = cache.getQueryData(["lichess-me"]) as { fair_play_lock: boolean } | undefined;
      if (me) cache.setQueryData(["lichess-me"], { ...me, fair_play_lock: (data as { locked: boolean }).locked });
      else refetch(["lichess-me"]);
      break;
    }
  }
}

/** How often a running test's game list is fetched again: the runner reads its whole PGN for it. */
export const GAMES_EVERY_MS = 15_000;
const gamesFetched = new Map<number, number>();

/**
 * A test reports after every game pair, a few times a second on a busy machine. Its detail takes the new totals and
 * chart point in place; its game list, which grows to thousands of games, is fetched again at most every
 * `GAMES_EVERY_MS`, and once more when the test ends.
 */
function applyTestProgress(cache: RunnerCache, test: Test) {
  const detail = cache.getQueryData(["test", test.id]) as TestDetail | undefined;
  if (detail && test.running) {
    const point = { games: test.games, llr: test.llr, nelo: test.estimate?.nelo };
    const points = detail.points.filter((p) => p.games < test.games);
    cache.setQueryData(["test", test.id], { test, points: [...points, point] });
  } else cache.invalidateQueries({ key: ["test", test.id] });

  const now = Date.now();
  if (!test.running || now - (gamesFetched.get(test.id) ?? -Infinity) >= GAMES_EVERY_MS) {
    gamesFetched.set(test.id, now);
    cache.invalidateQueries({ key: ["games", test.id] });
  }
  if (!test.running) gamesFetched.delete(test.id);
}

/** Folds a tune event into the cached list and detail, so the page follows a running tune without refetching. */
export function applyTuneEvent(cache: RunnerCache, kind: string, data: unknown) {
  const patchList = (id: number, patch: (t: Tune) => Tune) => {
    const list = cache.getQueryData(["tunes"]) as Tune[] | undefined;
    if (list?.some((t) => t.id === id)) {
      cache.setQueryData(
        ["tunes"],
        list.map((t) => (t.id === id ? patch(t) : t)),
      );
    } else cache.invalidateQueries({ key: ["tunes"] });
  };
  const patchDetail = (id: number, patch: (d: TuneDetail) => TuneDetail) => {
    const detail = cache.getQueryData(["tune", id]) as TuneDetail | undefined;
    if (detail) cache.setQueryData(["tune", id], patch(detail));
  };

  if (kind === "tune.updated") {
    const tune = data as Tune;
    patchList(tune.id, () => tune);
    patchDetail(tune.id, (d) => ({ ...d, tune }));
    cache.invalidateQueries({ key: ["tune", tune.id] });
  } else if (kind === "tune.progress") {
    const p = data as { id: number; iteration: number; values: number[]; result: number; penta: number[] };
    const move = (t: Tune): Tune => ({
      ...t,
      iteration: Math.max(t.iteration, p.iteration),
      params: t.params.map((param, i) => ({ ...param, value: p.values[i] ?? param.value })),
    });
    patchList(p.id, move);
    patchDetail(p.id, (d) =>
      d.history.some((h) => h.iteration === p.iteration)
        ? d
        : {
            tune: move(d.tune),
            history: [...d.history, { iteration: p.iteration, values: p.values, result: p.result, penta: p.penta }],
          },
    );
  } else if (kind === "tune.finished") {
    const f = data as { id: number; status: Tune["status"]; summary?: string | null; iteration: number };
    const end = (t: Tune): Tune => ({
      ...t,
      status: f.status,
      summary: f.summary ?? t.summary,
      iteration: f.iteration,
    });
    patchList(f.id, end);
    patchDetail(f.id, (d) => ({ ...d, tune: end(d.tune) }));
    cache.invalidateQueries({ key: ["tune", f.id] });
    cache.invalidateQueries({ key: ["tunes"] });
  }
}
