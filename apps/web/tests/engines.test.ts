import { beforeEach, describe, expect, it } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { dispatch } from "../src/api/stream";
import type { AppEvent, Engine, LevelsOverview } from "../src/api/types";
import { addErrorField, canAnalyse, engineDetail, readOptions, setupLine, sourceOf } from "../src/sheets/engines";
import { opponentChoices, opponentFamilies } from "../src/sheets/opponents";
import { useEngines } from "../src/stores/engines";

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
const stockfish = engine({
  id: "stockfish",
  name: "Stockfish",
  id_name: "Stockfish 17.1",
  analysis: true,
  levels: true,
});
const reckless = engine({ id: "reckless", name: "Reckless", id_name: "Reckless 0.9.0", levels: true });
const missing = engine({ id: "reckless", name: "Reckless", installed: false });
const custom = engine({ id: "berserk", name: "Berserk", kind: "custom", id_name: "Berserk 13", levels: true });
const fixed = engine({ id: "fixed", name: "Fixed", kind: "custom", rating: 1500, nodes: 1 });

const level = (id: number, nodes: number | null, nominal: number) => ({
  level: { id, name: `Level ${id}`, nodes, multipv: 1, temperature: 0, nominal },
  games: 40,
  rating: { elo: nominal, low: nominal - 40, high: nominal + 40 },
});
const levels = { levels: [level(1, 300, 800), level(12, null, 3000)], suggested: 1 } as LevelsOverview;

/** Each engine's ladder: Reckless's level 1 rated apart from Stockfish's. */
const reckless1 = { ...levels, engine: "reckless", suggested: 12 } as LevelsOverview;
const ladders: Record<string, LevelsOverview> = { stockfish: { ...levels, engine: "stockfish" }, reckless: reckless1 };
const ladderOf = (id: string) => ladders[id] ?? null;

describe("the engines New game offers", () => {
  it("lists every engine's own levels, the analysis engine first, then engines as they are", () => {
    const rows = opponentChoices([custom, reckless, stockfish, fixed], "stockfish", ladderOf);
    expect(rows.map((r) => r.value)).toEqual([
      "stockfish@1",
      "stockfish",
      "berserk",
      "reckless@1",
      "reckless",
      "fixed",
    ]);
    expect(rows.map((r) => r.family)).toEqual(["stockfish", "stockfish", "berserk", "reckless", "reckless", "fixed"]);
    expect(rows[0]).toMatchObject({ name: "Level 1", label: "Stockfish 1", near: true, disabled: false });
    expect(rows[1]).toMatchObject({ name: "Stockfish at full strength", label: "Stockfish" });
    // Berserk's ladder is not read yet: it is offered as it is.
    expect(rows[2]).toMatchObject({ text: "Berserk 13", label: "Berserk" });
    expect(rows[3]).toMatchObject({ name: "Level 1", label: "Reckless 1", near: false });
    expect(rows[4]).toMatchObject({ name: "Reckless at full strength", near: true });
    expect(rows[5]).toMatchObject({ text: "≈ 1,500", label: "Fixed" });
  });

  it("makes one family per engine: strengths for an engine with levels, none for an engine alone", () => {
    const rows = opponentChoices([custom, reckless, stockfish, fixed], "stockfish", ladderOf);
    const families = opponentFamilies(rows, [custom, reckless, stockfish, fixed]);
    expect(families.map((f) => [f.key, f.name, f.choices.length, f.disabled])).toEqual([
      ["stockfish", "Stockfish", 2, false],
      ["berserk", "Berserk", 1, false],
      ["reckless", "Reckless", 2, false],
      ["fixed", "Fixed", 1, false],
    ]);
    expect(families[2]!.detail).toBe("Levels, then full strength");
    // Without MultiPV, the levels are node budgets alone.
    const plain = { ...custom, multipv: false };
    const own = opponentFamilies(
      opponentChoices([plain], "stockfish", () => levels),
      [plain],
    );
    expect(own[0]).toMatchObject({ key: "berserk", detail: "Levels by node budget alone, then full strength" });
    const none = opponentFamilies(opponentChoices([missing], "stockfish", ladderOf));
    expect(none).toMatchObject([{ key: "reckless", disabled: true, detail: "Not installed" }]);
  });

  it("offers an engine as it is while its levels are not known, and disabled when it is missing", () => {
    expect(opponentChoices([stockfish], "stockfish", () => null).map((r) => r.value)).toEqual(["stockfish"]);
    const gone = opponentChoices([{ ...stockfish, installed: false }], "stockfish", ladderOf);
    expect(gone).toMatchObject([{ value: "stockfish", disabled: true }]);
  });
});

describe("the engines in Settings", () => {
  it("reads UCI options one Name=value a line", () => {
    expect(readOptions("Threads=4\n\n Hash = 256 \nSyzygyPath=C:\\tb=x")).toEqual({
      options: { Threads: "4", Hash: "256", SyzygyPath: "C:\\tb=x" },
    });
    expect(readOptions("Threads 4").error).toBe("Line 1 is not Name=value.");
    expect(readOptions("").options).toEqual({});
  });

  it("says what an engine reports, and which can analyse", () => {
    expect(engineDetail(stockfish)).toBe("Stockfish 17.1");
    expect(engineDetail({ ...reckless, id_name: null, path: "tools\\reckless\\r.exe" })).toBe("tools\\reckless\\r.exe");
    expect(engineDetail(missing)).toBe("Installed by setup, below");
    expect(engineDetail({ ...custom, installed: false, path: "C:\\e\\berserk.exe" })).toBe("C:\\e\\berserk.exe");
    expect(engineDetail({ ...custom, multipv: false })).toBe(
      "Berserk 13 · No MultiPV: its levels play their node budget alone",
    );
    expect(engineDetail({ ...fixed, id_name: "Fixed 1" })).toBe("Fixed 1 · 1 node a move, no levels");
    expect(
      sourceOf("https://github.com/codedeliveryservice/Reckless/releases/download/v0.9.0/reckless-windows-avx2.exe"),
    ).toEqual({ host: "github.com", file: "reckless-windows-avx2.exe" });
    expect(addErrorField("There is already an engine berserk.")).toBe("name");
    expect(addErrorField("C:\\x.exe did not answer as a UCI engine.")).toBe("path");
    expect(canAnalyse(stockfish)).toBe(true);
    expect(canAnalyse(custom)).toBe(true);
    expect(canAnalyse(reckless)).toBe(true);
    expect(canAnalyse(missing)).toBe(false);
    expect(canAnalyse(fixed)).toBe(false);
  });

  it("says what setup would download", () => {
    const sf = { package: "stockfish" as const, name: "Stockfish 19", url: "u", bytes: 75_000_000 };
    const rk = { package: "reckless" as const, name: "Reckless 0.9.0", url: "v", bytes: 65_000_000 };
    expect(setupLine({ downloads: [sf, rk], bytes: 140_000_000, installed: [], notes: [] })).toBe(
      "Stockfish 19 and Reckless 0.9.0: 140 MB",
    );
    expect(setupLine({ downloads: [], bytes: 0, installed: ["stockfish"], notes: [] })).toBe(
      "Everything setup installs is here.",
    );
  });
});

describe("the engines store", () => {
  beforeEach(() => setActivePinia(createPinia()));

  it("takes the list from engines.changed, and knows the analysis engine", () => {
    const e = useEngines();
    expect(e.analysisId).toBe("stockfish");
    dispatch({
      id: 1,
      topic: "engines",
      kind: "engines.changed",
      data: { analysis: "berserk", engines: [stockfish, custom] },
    } as AppEvent);
    expect(e.loaded).toBe(true);
    expect(e.analysisId).toBe("berserk");
    expect(e.analysis?.name).toBe("Berserk");
    expect(e.byId("stockfish")?.id_name).toBe("Stockfish 17.1");
  });
});
