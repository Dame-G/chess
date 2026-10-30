import { afterEach, describe, expect, it } from "vitest";
import type { Engine, LevelsOverview } from "../../src/api/types";
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
  multipv: true,
  installed: true,
  analysis: false,
  levels: false,
  ...e,
});
const stockfish = engine({ id: "stockfish", name: "Stockfish", analysis: true });
const reckless = engine({ id: "reckless", name: "Reckless", levels: true });
const gone = engine({ id: "komodo", name: "Komodo", kind: "custom", installed: false });
const berserk = engine({ id: "berserk", name: "Berserk", kind: "custom" });
const level = (id: number, nodes: number | null, nominal: number) => ({
  level: { id, name: `Level ${id}`, nodes, multipv: 1, temperature: 0, nominal },
  games: 0,
});
/** Reckless's ladder is read (level 2 nearest you); Stockfish's is not, so it plays as it is. */
const ladder = {
  engine: "reckless",
  levels: [level(1, 300, 800), level(2, 500, 1000), level(12, null, 3000)],
  suggested: 2,
};
const choices = opponentChoices([stockfish, reckless, berserk, gone], "stockfish", (id) =>
  id === "reckless" ? (ladder as LevelsOverview) : null,
);

describe("OpponentPicker", () => {
  it("chooses the engine first and then its strength, and remembers the strength in each family", async () => {
    const seen: string[] = [];
    m = await mount(OpponentPicker, {
      modelValue: "stockfish",
      choices,
      engines: [stockfish, reckless, berserk, gone],
      suggested: "stockfish",
      "onUpdate:modelValue": (v: string) => seen.push(v),
    });
    const first = m.el.querySelectorAll("[role=radiogroup]")[0]!;
    const families = [...first.querySelectorAll<HTMLButtonElement>("[role=radio]")];
    expect(families.map((b) => text(b))).toEqual(["Stockfish", "Reckless", "Berserk", "Komodo"]);
    // The analysis engine's levels are not known here, so it plays as it is: no second step.
    expect(m.el.querySelectorAll("[role=radiogroup]").length).toBe(1);

    // Reckless starts at its own level nearest you.
    families[1]!.click();
    await m.flush();
    expect(seen).toEqual(["reckless@2"]);

    // The arrow keys move through the families and choose, as in any radio group.
    key(families[0]!, "ArrowRight");
    await m.flush();
    expect(seen).toEqual(["reckless@2", "reckless@2"]);
  });

  it("offers an engine's levels as the strength step and names what is missing", async () => {
    const seen: string[] = [];
    m = await mount(OpponentPicker, {
      modelValue: "reckless@1",
      choices,
      engines: [stockfish, reckless, berserk, gone],
      suggested: "stockfish",
      "onUpdate:modelValue": (v: string) => seen.push(v),
    });
    const groups = [...m.el.querySelectorAll("[role=radiogroup]")];
    expect(groups.length).toBe(2);
    const rows = [...groups[1]!.querySelectorAll<HTMLButtonElement>("[role=radio]")];
    expect(rows.length).toBe(3);
    expect(m.el.textContent).toContain("Strength");
    expect(text(m.el.querySelector(".miss"))).toContain("Komodo is not installed");
    expect(m.el.querySelector(".miss a")?.getAttribute("href")).toBe("/settings?tab=engines");
    [...groups[0]!.querySelectorAll<HTMLButtonElement>("[role=radio]")][2]!.click();
    await m.flush();
    expect(seen).toEqual(["berserk"]);
  });
});
