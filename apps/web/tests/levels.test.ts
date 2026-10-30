import { describe, expect, it } from "vitest";
import type { LevelsOverview } from "../src/api/types";
import { ladder } from "../src/sheets/levels";
import { mount } from "./ui/mount";
import UiMeter from "../src/ui/UiMeter.vue";

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

  it("keeps the levels in order and puts you between the two your rating falls under and over", () => {
    expect(ladder(overview).map((r) => r.id)).toEqual(["level-1", "level-2", "you", "level-3"]);
  });

  it("marks a rating that was never played as an estimate and the level nearest you", () => {
    const rungs = ladder(overview);
    expect(rungs[0]).toMatchObject({ standing: "estimate", text: "≈ 800", games: 0, low: null, high: null });
    expect(rungs[1]).toMatchObject({ standing: "found", text: "968 ± 52", low: 916, high: 1020 });
    // Ten games: a rating, but provisional, written with its margin.
    expect(rungs[3]).toMatchObject({ near: true, standing: "provisional", text: "≈ 1,181 ± 47" });
  });

  it("leaves a level where it is when its provisional rating falls below the level under it", () => {
    const odd = {
      ...overview,
      levels: [
        { level: level(1, 800), games: 40, rating: { elo: 1500, low: 1400, high: 1600 } },
        { level: level(2, 1000), games: 4, rating: { elo: 1300, low: 800, high: 1800 } },
      ],
      you: null,
    } as unknown as LevelsOverview;
    expect(ladder(odd).map((r) => r.id)).toEqual(["level-1", "level-2"]);
  });

  it("puts you at the top when you are above every level", () => {
    const high = { ...overview, you: { estimate: { elo: 2500, low: 2400, high: 2600 }, games: 3 } } as LevelsOverview;
    expect(ladder(high).at(-1)?.id).toBe("you");
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
