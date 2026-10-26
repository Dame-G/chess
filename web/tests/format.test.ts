import { describe, expect, it } from "vitest";
import type { Experiment, Test } from "../src/api/queries";
import { duration, ending, failure, forecast, signed, speed, verdict, winChance } from "../src/format";

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
  it("names the outcome with the shared words", () => {
    expect(verdict(e({})).label).toBe("Passed");
    expect(verdict(e({ status: "failed" })).label).toBe("Rejected");
    expect(verdict(e({ kind: "nonregression" })).label).toBe("Passed");
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

describe("rounding", () => {
  it("gives no sign to a figure that rounds to zero", () => {
    expect(signed(-0.04, 1)).toBe("0.0");
    expect(signed(0.004)).toBe("0.00");
    expect(signed(-0)).toBe("0.00");
    expect(signed(0.05, 1)).toBe("+0.1");
  });

  it("carries a rounded minute or thousand into the next unit", () => {
    expect(duration(59.6)).toBe("1 h");
    expect(duration(119.7)).toBe("2 h");
    expect(duration(0.5)).toBe("under a minute");
    expect(speed(999_600)).toBe("1.00M nodes/s");
    expect(speed(512_300)).toBe("512k nodes/s");
  });
});

describe("failure", () => {
  it("hides file paths on either system, but not branch names", () => {
    const hidden = "It failed. The runner's log has the details.";
    expect(failure("cannot open C:\\Users\\x\\engine.exe")).toBe(hidden);
    expect(failure("cannot open /home/dame/reckless/target")).toBe(hidden);
    expect(failure("cannot open '~/dev/reckless'")).toBe(hidden);
    expect(failure("bench differs on test/no-nmp")).toBe("bench differs on test/no-nmp");
    expect(failure("no commit named test/x in /home/dame/reckless")).toBe(
      "There is no branch, tag or commit called test/x.",
    );
  });
});
