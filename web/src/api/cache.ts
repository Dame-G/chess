import type { EntryKey } from "@pinia/colada";
import type { RunnerEvent } from "./events";
import type { Job, Tune, TuneDetail, Version } from "./queries";

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
      const { status } = data as Pick<Job, "status">;
      // A finished job may have made a build, a branch (an applied tune) or a verdict.
      if (status === "done") refetch(["builds"], ["branches"], ["versions"]);
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
    case "test.progress": {
      const { id } = data as { id: number };
      refetch(["test", id], ["games", id]);
      break;
    }
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
  }
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
