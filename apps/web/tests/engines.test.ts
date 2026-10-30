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
const maia1500 = engine({ id: "maia-1500", name: "Maia 1500", rating: 1500, nodes: 1 });
const maia1100 = engine({ id: "maia-1100", name: "Maia 1100", rating: 1100, nodes: 1, installed: false });
const custom = engine({ id: "berserk", name: "Berserk", kind: "custom", id_name: "Berserk 13" });

const level = (id: number, nodes: number | null, nominal: number) => ({
  level: { id, name: `Level ${id}`, nodes, multipv: 1, temperature: 0, nominal },
  games: 40,
  rating: { elo: nominal, low: nominal - 40, high: nominal + 40 },
});
const levels = { levels: [level(1, 300, 800), level(12, null, 3000)], suggested: 1 } as LevelsOverview;

describe("the engines New game offers", () => {
  it("lists the analysis engine's levels, then the human-like engines by rating, then the rest", () => {
    const rows = opponentChoices([custom, maia1500, stockfish, maia1100], "stockfish", levels);
    expect(rows.map((r) => r.value)).toEqual(["stockfish@1", "stockfish", "maia-1100", "maia-1500", "berserk"]);
    expect(rows.map((r) => r.family)).toEqual(["stockfish", "stockfish", "human-like", "human-like", "berserk"]);
    expect(rows[0]).toMatchObject({ name: "Level 1", label: "Stockfish 1", near: true, disabled: false });
    expect(rows[1]).toMatchObject({ name: "Stockfish at full strength", label: "Stockfish" });
    expect(rows[2]).toMatchObject({ text: "Not installed", disabled: true });
    expect(rows[3]).toMatchObject({ name: "Maia 1500", label: "Maia 1500", text: "", disabled: false });
    expect(rows[4]).toMatchObject({ text: "Berserk 13", label: "Berserk" });
  });

  it("groups them into families for the first step: strengths for Stockfish and Maia, none for an engine alone", () => {
    const rows = opponentChoices([custom, maia1500, stockfish, maia1100], "stockfish", levels);
    const families = opponentFamilies(rows, "stockfish");
    expect(families.map((f) => [f.key, f.name, f.choices.length, f.disabled])).toEqual([
      ["stockfish", "Stockfish", 2, false],
      ["human-like", "Maia", 2, false],
      ["berserk", "Berserk", 1, false],
    ]);
    const none = opponentFamilies(opponentChoices([maia1100], "stockfish", null), "stockfish");
    expect(none).toMatchObject([{ key: "human-like", disabled: true }]);
  });

  it("offers the analysis engine as it is while its levels are not known, and disabled when it is missing", () => {
    expect(opponentChoices([stockfish], "stockfish", null).map((r) => r.value)).toEqual(["stockfish"]);
    const missing = opponentChoices([{ ...stockfish, installed: false }], "stockfish", levels);
    expect(missing).toMatchObject([{ value: "stockfish", disabled: true }]);
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
    expect(engineDetail(maia1500)).toBe("");
    expect(engineDetail({ ...maia1500, path: "tools\\lc0\\lc0.exe" })).toBe("tools\\lc0\\lc0.exe");
    expect(engineDetail(maia1100)).toBe("Installed by setup, below");
    expect(engineDetail({ ...custom, installed: false, path: "C:\\e\\berserk.exe" })).toBe("C:\\e\\berserk.exe");
    expect(sourceOf("https://github.com/CSSLab/maia-chess/releases/download/v1.0/maia-1100.pb.gz")).toEqual({
      host: "github.com",
      file: "maia-1100.pb.gz",
    });
    expect(addErrorField("There is already an engine berserk.")).toBe("name");
    expect(addErrorField("C:\\x.exe did not answer as a UCI engine.")).toBe("path");
    expect(canAnalyse(stockfish)).toBe(true);
    expect(canAnalyse(custom)).toBe(true);
    expect(canAnalyse(maia1500)).toBe(false);
    expect(canAnalyse(maia1100)).toBe(false);
  });

  it("says what setup would download", () => {
    const d = (name: string) => ({ package: "maia" as const, name, url: "u", bytes: 1_000_000 });
    expect(
      setupLine({ downloads: [d("lc0"), d("Maia 1100"), d("Maia 1900")], bytes: 3_000_000, installed: [], notes: [] }),
    ).toBe("lc0, Maia 1100 and Maia 1900: 3.0 MB");
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
