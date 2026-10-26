import { describe, expect, it } from "vitest";
import { applyTuneEvent } from "../src/api/cache";
import type { Tune, TuneDetail } from "../src/api/queries";
import { change, eta, fmt, normalised, pairs, resumable, trajectory, tuneLine } from "../src/components/tune/tune";
import list from "./fixtures/tunes.json";
import detail from "./fixtures/tune-1.json";

const tunes = list as unknown as Tune[];
const first = detail as unknown as TuneDetail;

/** A query cache that is only a map, so the events' effect can be read back. */
function cache(entries: Record<string, unknown>) {
  const store = new Map(Object.entries(entries).map(([k, v]) => [k, structuredClone(v)]));
  const invalidated: string[] = [];
  return {
    store,
    invalidated,
    getQueryData: (key: readonly unknown[]) => store.get(JSON.stringify(key)),
    setQueryData: (key: readonly unknown[], data: unknown) => void store.set(JSON.stringify(key), data),
    invalidateQueries: (filters?: { key: readonly unknown[] }) => void invalidated.push(JSON.stringify(filters?.key)),
  };
}

describe("tune events", () => {
  it("appends a progress event to the history and moves the parameters", () => {
    const c = cache({ '["tunes"]': tunes, '["tune",1]': first });
    const values = first.tune.params.map((p) => p.value + 1);
    applyTuneEvent(c, "tune.progress", { id: 1, iteration: 25, values, result: 3, penta: [0, 1, 2, 1, 0] });
    const d = c.store.get('["tune",1]') as TuneDetail;
    expect(d.history).toHaveLength(first.history.length + 1);
    expect(d.history.at(-1)).toMatchObject({ iteration: 25, result: 3 });
    expect(d.tune.iteration).toBe(25);
    expect(d.tune.params[0]!.value).toBe(values[0]);
    expect((c.store.get('["tunes"]') as Tune[])[0]!.iteration).toBe(25);
  });

  it("ignores an iteration it already has", () => {
    const c = cache({ '["tunes"]': tunes, '["tune",1]': first });
    applyTuneEvent(c, "tune.progress", { id: 1, iteration: 24, values: [], result: 0, penta: [] });
    expect((c.store.get('["tune",1]') as TuneDetail).history).toHaveLength(first.history.length);
  });

  it("marks a finished tune and refreshes the lists", () => {
    const c = cache({ '["tunes"]': tunes, '["tune",1]': first });
    applyTuneEvent(c, "tune.finished", { id: 1, status: "done", summary: "1000 iterations", iteration: 1000 });
    expect((c.store.get('["tune",1]') as TuneDetail).tune).toMatchObject({ status: "done", iteration: 1000 });
    expect(c.invalidated).toContain('["tunes"]');
  });

  it("refetches the list when it meets a tune it does not know", () => {
    const c = cache({});
    applyTuneEvent(c, "tune.updated", { ...tunes[0]!, id: 9 });
    expect(c.invalidated).toContain('["tunes"]');
  });
});

describe("tune figures", () => {
  const p = { start: 80, min: 40, max: 160 };

  it("measures movement as a share of the range", () => {
    expect(normalised(p, 80)).toBe(0);
    expect(normalised(p, 100)).toBeCloseTo(16.67, 1);
    expect(change(p, 92.4)).toMatchObject({ delta: 12, text: "+12" });
    expect(change(p, 60).text).toBe("−20");
  });

  it("shows whole numbers for integer parameters", () => {
    expect(fmt(p, 91.6)).toBe("92");
    expect(fmt({ start: 0.5, min: 0, max: 1 }, 0.7234)).toBe("0.72");
  });

  it("starts the trajectory at the declared values", () => {
    const t = trajectory(first.tune.params, first.history);
    expect(t.x[0]).toBe(0);
    expect(t.x).toHaveLength(first.history.length + 1);
    expect(t.scaled.every((s) => s[0] === 0)).toBe(true);
  });

  it("folds the pentanomial into pairs won, drawn and lost", () => {
    expect(pairs([1, 2, 3, 4, 5])).toEqual({ win: 9, draw: 3, loss: 3, total: 15 });
  });

  it("says how a tune stands", () => {
    expect(tuneLine(tunes[0]!)).toContain("24 of 1,000 iterations");
    expect(tuneLine(tunes[1]!)).toContain("it can resume");
    expect(resumable(tunes[1]!)).toBe(true);
    expect(resumable(tunes[0]!)).toBe(false);
  });

  it("estimates the time left only after watching a while", () => {
    expect(eta([{ iteration: 1, at: 0 }], 100)).toBeUndefined();
    expect(
      eta(
        [
          { iteration: 10, at: 0 },
          { iteration: 20, at: 60_000 },
        ],
        80,
      ),
    ).toBe("6 min");
  });
});
