import { afterEach, describe, expect, it } from "vitest";
import type { Engine } from "../../src/api/types";
import OpponentPicker from "../../src/parts/OpponentPicker.vue";
import { opponentChoices } from "../../src/sheets/opponents";
import { key, mount, text, type Mounted } from "./mount";

let m: Mounted | undefined;
afterEach(() => {
  m?.unmount();
  m = undefined;
});

const engine = (e: Partial<Engine> & Pick<Engine, "id" | "name">): Engine => ({
  kind: "builtin",
  args: [],
  options: {},
  installed: true,
  analysis: false,
  levels: false,
  ...e,
});
const stockfish = engine({ id: "stockfish", name: "Stockfish", analysis: true });
const maia1100 = engine({ id: "maia-1100", name: "Maia 1100", rating: 1100, nodes: 1, installed: false });
const maia1500 = engine({ id: "maia-1500", name: "Maia 1500", rating: 1500, nodes: 1 });
const berserk = engine({ id: "berserk", name: "Berserk", kind: "custom" });
const choices = opponentChoices([stockfish, maia1100, maia1500, berserk], "stockfish", null);

describe("OpponentPicker", () => {
  it("chooses the engine first and then its strength, and remembers the strength in each family", async () => {
    const seen: string[] = [];
    m = await mount(OpponentPicker, {
      modelValue: "stockfish",
      choices,
      analysis: "stockfish",
      suggested: "stockfish",
      "onUpdate:modelValue": (v: string) => seen.push(v),
    });
    const first = m.el.querySelectorAll("[role=radiogroup]")[0]!;
    const families = [...first.querySelectorAll<HTMLButtonElement>("[role=radio]")];
    expect(families.map((b) => text(b))).toEqual(["Stockfish", "Maia", "Berserk"]);
    // The analysis engine's levels are not known here, so it plays as it is: no second step.
    expect(m.el.querySelectorAll("[role=radiogroup]").length).toBe(1);

    // Maia starts at its first installed rating; the not installed 1,100 is skipped.
    families[1]!.click();
    await m.flush();
    expect(seen).toEqual(["maia-1500"]);

    // The arrow keys move through the families and choose, as in any radio group.
    key(families[0]!, "ArrowRight");
    await m.flush();
    expect(seen).toEqual(["maia-1500", "maia-1500"]);
  });

  it("starts a family at its first installed strength and names what is missing", async () => {
    const seen: string[] = [];
    m = await mount(OpponentPicker, {
      modelValue: "maia-1500",
      choices,
      analysis: "stockfish",
      suggested: "stockfish",
      "onUpdate:modelValue": (v: string) => seen.push(v),
    });
    const groups = [...m.el.querySelectorAll("[role=radiogroup]")];
    expect(groups.length).toBe(2);
    const rows = [...groups[1]!.querySelectorAll<HTMLButtonElement>("[role=radio]")];
    expect(rows.map((r) => r.disabled)).toEqual([true, false]);
    expect(text(m.el.querySelector(".miss"))).toContain("need setting up");
    expect(m.el.querySelector(".miss a")?.getAttribute("href")).toBe("/settings?tab=engines");
    [...groups[0]!.querySelectorAll<HTMLButtonElement>("[role=radio]")][2]!.click();
    await m.flush();
    expect(seen).toEqual(["berserk"]);
  });
});
