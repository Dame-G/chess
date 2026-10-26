import { describe, expect, it } from "vitest";
import { applyRunnerEvent } from "../src/api/cache";
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
  ])("%s %j refetches %j", (kind, data, keys) => {
    expect(after(kind, data).refetched).toEqual(keys);
  });

  it("takes the Lichess status as it comes, without a request", () => {
    const status = { online: true };
    const c = after("lichess.status", status);
    expect(c.store.get('["lichess"]')).toBe(status);
    expect(c.refetched).toEqual([]);
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
});
