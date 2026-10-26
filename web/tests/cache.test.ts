import { describe, expect, it, vi } from "vitest";
import { applyRunnerEvent, GAMES_EVERY_MS } from "../src/api/cache";
import type { Version } from "../src/api/queries";

/** A query cache that is only a map, recording what was refetched ("*" for everything). */
function cache(entries: Record<string, unknown> = {}) {
  const store = new Map(Object.entries(entries));
  const refetched: string[] = [];
  return {
    store,
    refetched,
    getQueryData: (key: readonly unknown[]) => store.get(JSON.stringify(key)),
    setQueryData: (key: readonly unknown[], data: unknown) => void store.set(JSON.stringify(key), data),
    invalidateQueries: (filters?: { key: readonly unknown[] }) =>
      void refetched.push(filters ? JSON.stringify(filters.key) : "*"),
  };
}

const after = (kind: string, data: unknown, entries?: Record<string, unknown>) => {
  const c = cache(entries);
  applyRunnerEvent(c, { id: 1, kind, data });
  return c;
};

describe("applyRunnerEvent", () => {
  it.each([
    ["runner.started", {}, ["*"]],
    ["job.updated", { id: 4, status: "running" }, ['["jobs"]']],
    ["job.updated", { id: 4, status: "done" }, ['["jobs"]', '["builds"]', '["branches"]', '["versions"]']],
    ["job.updated", { id: 4, status: "failed" }, ['["jobs"]', '["versions"]']],
    ["versions.changed", { reason: "hidden" }, ['["versions"]', '["branches"]', '["builds"]']],
    ["experiment.updated", { id: 3 }, ['["experiments"]', '["experiment",3]', '["versions"]']],
    ["test.progress", { id: 2 }, ['["test",2]', '["games",2]']],
    ["match.started", { id: 7 }, ['["matches"]']],
    ["match.ended", { id: 7 }, ['["matches"]']],
    ["match.move", { id: 7 }, []],
    ["match.info", { id: 7 }, []],
    ["job.log", { id: 4, line: "x" }, []],
    ["analysis.info", { session: 1 }, []],
    ["lichess.challenge", { id: "c" }, []],
    ["lichess.me.challenge", { event: "created" }, ['["lichess-me"]']],
    [
      "job.updated",
      { id: 5, status: "done", spec: { kind: "lichess_import" } },
      ['["jobs"]', '["builds"]', '["branches"]', '["versions"]', '["matches"]'],
    ],
  ])("%s %j refetches %j", (kind, data, keys) => {
    expect(after(kind, data).refetched).toEqual(keys);
  });

  it("takes the Lichess status as it comes, without a request", () => {
    const status = { online: true };
    const c = after("lichess.status", status);
    expect(c.store.get('["lichess"]')).toBe(status);
    expect(c.refetched).toEqual([]);
  });

  it("takes your Lichess status and fair-play lock as they come", () => {
    const status = { configured: true, fair_play_lock: false };
    const c = after("lichess.me.status", status);
    expect(c.store.get('["lichess-me"]')).toBe(status);
    expect(c.refetched).toEqual([]);

    const locked = after("lichess.me.fair_play", { locked: true }, { '["lichess-me"]': status });
    expect(locked.store.get('["lichess-me"]')).toEqual({ configured: true, fair_play_lock: true });
    expect(after("lichess.me.fair_play", { locked: true }).refetched).toEqual(['["lichess-me"]']);
  });

  it("refreshes every games list, yours and the bot's, when a match starts or ends", () => {
    expect(after("match.started", { id: 8, match: { lichess: { account: "me" } } }).refetched).toEqual(['["matches"]']);
    expect(after("match.ended", { id: 8 }).refetched).toEqual(['["matches"]']);
  });

  it("refreshes versions when a tune changes state, and patches its iteration as it runs", () => {
    expect(after("tune.finished", { id: 1, status: "done", iteration: 9 }).refetched).toContain('["versions"]');
    expect(after("tune.updated", { id: 1, status: "running", params: [] }).refetched).toContain('["versions"]');

    const versions = [
      { name: "spsa/tunables", tune: { id: 1, iteration: 3, iterations: 10, status: "running" } },
      { name: "main", tune: null },
    ] as unknown as Version[];
    const c = after(
      "tune.progress",
      { id: 1, iteration: 4, values: [], result: 0, penta: [] },
      {
        '["versions"]': versions,
      },
    );
    expect(c.refetched).not.toContain('["versions"]');
    const patched = c.store.get('["versions"]') as Version[];
    expect(patched[0]!.tune).toMatchObject({ id: 1, iteration: 4, iterations: 10 });
    expect(patched[1]).toBe(versions[1]);
  });

  it("folds a running test's progress into its detail and fetches its games at most every few seconds", () => {
    vi.useFakeTimers();
    try {
      const detail = { test: { id: 9, games: 2, running: true }, points: [{ games: 2, llr: 0.1, nelo: 1 }] };
      const progress = (games: number, running = true) => ({
        id: 9,
        games,
        running,
        llr: games / 10,
        estimate: { nelo: games },
      });
      const c = cache({ '["test",9]': detail });
      applyRunnerEvent(c, { id: 1, kind: "test.progress", data: progress(4) });
      expect(c.store.get('["test",9]')).toMatchObject({
        test: { games: 4 },
        points: [{ games: 2 }, { games: 4, llr: 0.4, nelo: 4 }],
      });
      expect(c.refetched).toEqual(['["games",9]']);

      applyRunnerEvent(c, { id: 2, kind: "test.progress", data: progress(6) });
      expect(c.refetched).toEqual(['["games",9]']);
      vi.advanceTimersByTime(GAMES_EVERY_MS);
      applyRunnerEvent(c, { id: 3, kind: "test.progress", data: progress(8) });
      expect(c.refetched).toEqual(['["games",9]', '["games",9]']);

      // The end: the detail and the games are fetched as the runner has them.
      applyRunnerEvent(c, { id: 4, kind: "test.progress", data: progress(8, false) });
      expect(c.refetched.slice(2)).toEqual(['["test",9]', '["games",9]']);
    } finally {
      vi.useRealTimers();
    }
  });
});
