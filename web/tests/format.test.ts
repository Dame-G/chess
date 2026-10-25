import { describe, expect, it } from "vitest";
import type { Experiment, Test } from "../src/api/queries";
import { duration, ending, forecast, signed, verdict, winChance } from "../src/format";

const test = (over: Partial<Test>): Test =>
  ({
    created_at: "2026-10-25T18:00:00Z",
    games: 1000,
    llr: -1.125,
    running: true,
    spec: { sprt: { elo0: -5, elo1: 0, alpha: 0.05, beta: 0.1 } },
    ...over,
  }) as Test;

describe("forecast", () => {
  it("leans towards the line the evidence is drifting to, and times the rest at the pace so far", () => {
    // Reject line is ln(0.1 / 0.95) = -2.25: half way after 1000 games in 10 minutes leaves 1000 games, 10 minutes.
    const f = forecast(test({}), new Date("2026-10-25T18:10:00Z").getTime())!;
    expect(f.toward).toBe("reject");
    expect(f.progress).toBeCloseTo(0.5, 2);
    expect(f.minutes).toBeCloseTo(10, 0);
  });

  it("gives no time while the evidence is too thin", () => {
    expect(forecast(test({ llr: 0.05 }))?.minutes).toBeUndefined();
    expect(forecast(test({ spec: {} as Test["spec"] }))).toBeUndefined();
  });
});

describe("verdict", () => {
  const e = (over: Partial<Experiment>) => ({ kind: "gainer", status: "passed", summary: "", ...over }) as Experiment;
  it("says what a result means for the kind of change", () => {
    expect(verdict(e({})).label).toBe("Stronger");
    expect(verdict(e({ status: "failed" })).label).toBe("Not stronger");
    expect(verdict(e({ kind: "nonregression" })).label).toBe("No weaker");
    expect(verdict(e({ status: "running", current: "screen" })).sentence).toBe("Quick screen in progress.");
  });
});

describe("words", () => {
  it("formats for people", () => {
    expect(signed(-4.26, 1)).toBe("−4.3");
    expect(signed(0)).toBe("0.00");
    expect(duration(95)).toBe("1 h 35 min");
    expect(ending("adjudication")).toBe("Called early");
    expect(winChance(0)).toBe(0.5);
  });
});
