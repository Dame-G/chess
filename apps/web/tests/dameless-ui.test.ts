import { describe, expect, it } from "vitest";
import type { Idea, LevelsOverview, TrialSummary } from "../src/api/types";
import { evidenceLine, gaugeTrial, eloFigure } from "../src/sheets/idea";
import { ladder } from "../src/sheets/levels";
import { datagenEstimate, datagenSummary } from "../src/sheets/training";
import { mount } from "./ui/mount";
import UiMeter from "../src/ui/UiMeter.vue";

const trial = (t: Partial<TrialSummary>): TrialSummary => ({
  id: 1,
  stage: "stc",
  status: "running",
  pairs: 0,
  ...t,
});

describe("the evidence line of an idea in a list", () => {
  it("says the Elo and the pairs it stands on", () => {
    const idea: Pick<Idea, "status" | "trials"> = {
      status: "running",
      trials: [trial({ pairs: 412, elo: { elo: 0.7, low: -5.2, high: 6.6 } })],
    };
    expect(evidenceLine(idea)).toBe("+0.7 ± 5.9 Elo after 412 pairs");
  });

  it("falls back to the pairs, then to the trial's own words, then to nothing", () => {
    expect(evidenceLine({ status: "running", trials: [trial({ pairs: 1 })] })).toBe("1 pair");
    expect(evidenceLine({ status: "running", trials: [trial({ detail: "Iteration 3 of 1,000." })] })).toBe(
      "Iteration 3 of 1,000.",
    );
    expect(evidenceLine({ status: "queued", trials: [] })).toBe("");
  });

  it("draws a gauge only for a running trial that has evidence", () => {
    const llr = { lower: -2.94, upper: 2.94, value: 0.6 };
    expect(gaugeTrial({ trials: [trial({ llr })] })?.llr).toEqual(llr);
    expect(gaugeTrial({ trials: [trial({ llr, status: "passed" })] })).toBeUndefined();
    expect(gaugeTrial({ trials: [trial({})] })).toBeUndefined();
  });

  it("keeps an estimate on one line with fixed spaces", () => {
    expect(eloFigure({ elo: 0.8, low: -1.3, high: 2.9 })).toBe("+0.8 ± 2.1");
  });
});

describe("the ladder of levels", () => {
  const level = (id: number, nominal: number) => ({
    id,
    name: `Level ${id}`,
    nodes: id * 1000,
    nominal,
    multipv: 1,
    temperature: 0,
  });
  const overview = {
    levels: [
      { level: level(2, 1000), games: 30, rating: { elo: 968, low: 916, high: 1020 } },
      { level: level(1, 800), games: 0, rating: null },
      { level: level(3, 1200), games: 10, rating: { elo: 1181, low: 1134, high: 1228 } },
    ],
    suggested: 3,
    you: { estimate: { elo: 1100, low: 1000, high: 1200 }, games: 14 },
  } as unknown as LevelsOverview;

  it("orders the rungs by rating and puts you between the two it falls under and over", () => {
    expect(ladder(overview).map((r) => r.id)).toEqual(["level-1", "level-2", "you", "level-3"]);
  });

  it("marks a rating that was never played as an estimate and the level nearest you", () => {
    const rungs = ladder(overview);
    expect(rungs[0]).toMatchObject({ found: false, text: "≈ 800", games: 0 });
    expect(rungs[1]).toMatchObject({ found: true, text: "968 ± 52" });
    expect(rungs[3]).toMatchObject({ near: true });
  });

  it("puts you at the top when you are above every level", () => {
    const high = { ...overview, you: { estimate: { elo: 2500, low: 2400, high: 2600 }, games: 3 } } as LevelsOverview;
    expect(ladder(high).at(-1)?.id).toBe("you");
  });
});

describe("the data generation summary", () => {
  it("is one short line with the size and the time", () => {
    expect(datagenSummary(datagenEstimate(20_000_000, 7, 7))).toBe("~86.0 MB · 4 h to 5 h");
  });
});

describe("UiMeter", () => {
  it("fills to the share of the whole and says it for a screen reader", async () => {
    const m = await mount(UiMeter, { label: "Data used", value: 1, max: 10, text: "1.0 of 10 GB" });
    const track = m.el.querySelector('[role="meter"]')!;
    expect(track.getAttribute("aria-valuenow")).toBe("1");
    expect(track.getAttribute("aria-valuetext")).toBe("1.0 of 10 GB");
    expect(m.el.querySelector<HTMLElement>(".ui-meter-fill")!.style.inlineSize).toBe("10%");
    m.unmount();
  });

  it("never fills past the whole", async () => {
    const m = await mount(UiMeter, { label: "Data used", value: 30, max: 10 });
    expect(m.el.querySelector<HTMLElement>(".ui-meter-fill")!.style.inlineSize).toBe("100%");
    m.unmount();
  });
});
