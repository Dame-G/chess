import { h, nextTick } from "vue";
import { afterEach, describe, expect, it } from "vitest";
import type { Seat } from "../../src/api/types";
import { healthTone, workTone } from "../../src/ui/badge";
import UiBadge from "../../src/ui/UiBadge.vue";
import UiEmpty from "../../src/ui/UiEmpty.vue";
import UiField from "../../src/ui/UiField.vue";
import UiKeyValue from "../../src/ui/UiKeyValue.vue";
import UiKv from "../../src/ui/UiKv.vue";
import UiLive from "../../src/ui/UiLive.vue";
import UiPanelHead from "../../src/ui/UiPanelHead.vue";
import UiPlayer from "../../src/ui/UiPlayer.vue";
import UiResult from "../../src/ui/UiResult.vue";
import UiScore from "../../src/ui/UiScore.vue";
import UiSection from "../../src/ui/UiSection.vue";
import UiStat from "../../src/ui/UiStat.vue";
import UiStats from "../../src/ui/UiStats.vue";
import UiToolbar from "../../src/ui/UiToolbar.vue";
import { liveGame, strangersGame, wonGame } from "../../src/ui/kit/data";
import { mount, text, type Mounted } from "./mount";

let m: Mounted | undefined;
afterEach(() => {
  m?.unmount();
  m = undefined;
});

describe("UiBadge", () => {
  it("is neutral by default and takes a tone, solid and a dot", async () => {
    m = await mount(UiBadge, {}, { default: () => "Queued" });
    const el = m.el.querySelector(".ui-badge");
    expect(text(el)).toBe("Queued");
    expect(el?.className).toContain("ui-badge--neutral");
    m.props["tone"] = "win";
    m.props["solid"] = true;
    m.props["dot"] = true;
    await m.flush();
    expect(el?.className).toContain("ui-badge--win");
    expect(el?.className).toContain("ui-badge--solid");
    expect(m.el.querySelector(".ui-badge-dot")?.getAttribute("aria-hidden")).toBe("true");
  });

  it("maps every status to a tone", () => {
    expect(workTone.paused).toBe("warn");
    expect(healthTone.fail).toEqual({ tone: "loss", word: "Missing" });
    expect(healthTone.ok.word).toBe("Ready");
    expect(Object.keys(workTone)).toHaveLength(6);
    expect(Object.keys(healthTone)).toHaveLength(4);
  });
});

describe("UiResult", () => {
  const mark = (el: HTMLElement) => el.querySelector(".ui-result-mark");

  it("is ink for a game you were not in", async () => {
    m = await mount(UiResult, { result: "1-0" });
    expect(text(mark(m.el))).toBe("1-0");
    expect(m.el.querySelector(".ui-result")?.className).not.toMatch(/--(win|loss|draw)/);
    expect(text(m.el.querySelector(".sr-only"))).toBe("White won");
  });

  it("reads from your side: 1-0 is a win as White and a loss as Black", async () => {
    m = await mount(UiResult, { result: "1-0", you: "white" });
    expect(m.el.querySelector(".ui-result")?.className).toContain("ui-result--win");
    expect(text(m.el.querySelector(".sr-only"))).toBe("You won");
    m.props["you"] = "black";
    await m.flush();
    expect(m.el.querySelector(".ui-result")?.className).toContain("ui-result--loss");
    expect(text(m.el.querySelector(".sr-only"))).toBe("You lost");
  });

  it("writes a draw as ½-½, muted, and a running game as *", async () => {
    m = await mount(UiResult, { result: "1/2-1/2", you: "white" });
    expect(text(mark(m.el))).toBe("½-½");
    expect(m.el.querySelector(".ui-result")?.className).toContain("ui-result--draw");
    m.props["result"] = "*";
    await m.flush();
    expect(text(mark(m.el))).toBe("*");
    expect(text(m.el.querySelector(".sr-only"))).toBe("In progress");
    m.props["result"] = null;
    await m.flush();
    expect(text(mark(m.el))).toBe("–");
  });

  it("writes letters and dots", async () => {
    m = await mount(UiResult, { result: "0-1", you: "white", format: "letter" });
    expect(text(mark(m.el))).toBe("L");
    m.props["format"] = "dot";
    await m.flush();
    expect(text(mark(m.el))).toBe("");
    expect(m.el.querySelector(".ui-result")?.className).toContain("ui-result--dot");
    m.props["format"] = "letter";
    m.props["you"] = null;
    await m.flush();
    expect(text(mark(m.el))).toBe("0-1");
  });
});

describe("UiScore", () => {
  const figure = (el: HTMLElement) => text(el.querySelector(".ui-score-text"));

  it("writes the figure with a sign and a pip for the side ahead, and no colour", async () => {
    m = await mount(UiScore, { score: { cp: 62 } });
    expect(figure(m.el)).toBe("+0.6");
    expect(m.el.querySelector(".ui-score-pip--white")).not.toBeNull();
    expect(m.el.querySelector(".ui-score")?.className).not.toMatch(/--(win|loss)/);
    m.props["score"] = { cp: -423 };
    await m.flush();
    expect(figure(m.el)).toBe("−4.2");
    expect(m.el.querySelector(".ui-score-pip--black")).not.toBeNull();
    expect(m.el.querySelector(".ui-score")?.className).not.toMatch(/--(win|loss)/);
  });

  it("has no pip when level and writes mates as #n", async () => {
    m = await mount(UiScore, { score: { cp: 0 } });
    expect(figure(m.el)).toBe("0.0");
    expect(m.el.querySelector(".ui-score-pip")).toBeNull();
    expect(text(m.el.querySelector(".sr-only"))).toBe("Level");
    m.props["score"] = { mate: 3 };
    await m.flush();
    expect(figure(m.el)).toBe("#3");
    expect(text(m.el.querySelector(".sr-only"))).toBe("White mates (#3)");
  });

  it("takes outcome colours from the mover's side, with no pip", async () => {
    m = await mount(UiScore, { score: { cp: 120 }, perspective: "mover" });
    expect(m.el.querySelector(".ui-score")?.className).toContain("ui-score--win");
    expect(m.el.querySelector(".ui-score-pip")).toBeNull();
    m.props["score"] = { cp: -120 };
    await m.flush();
    expect(m.el.querySelector(".ui-score")?.className).toContain("ui-score--loss");
    m.props["score"] = { cp: 0 };
    await m.flush();
    expect(m.el.querySelector(".ui-score")?.className).not.toMatch(/--(win|loss)/);
  });

  it("writes a missing score as an en dash", async () => {
    m = await mount(UiScore, { score: null });
    expect(figure(m.el)).toBe("–");
    expect(text(m.el.querySelector(".sr-only"))).toBe("No score");
  });
});

describe("UiPlayer", () => {
  const engineSeat: Seat = { player: "engine:stockfish", name: "Stockfish 17.1", rating: 3290 };

  it("names an engine's seat as the server does and shows its rating", async () => {
    m = await mount(UiPlayer, { seat: engineSeat });
    expect(text(m.el.querySelector(".ui-player-name"))).toBe("Stockfish 17.1");
    expect(text(m.el.querySelector(".ui-player-rating"))).toBe("3,290");
  });

  it("marks a provisional rating with ? and an override with null shows none", async () => {
    m = await mount(UiPlayer, { seat: engineSeat, rating: 1345, provisional: true });
    expect(text(m.el.querySelector(".ui-player-rating"))).toBe("1,345?");
    m.props["rating"] = null;
    await m.flush();
    expect(m.el.querySelector(".ui-player-rating")).toBeNull();
  });

  it("shows a title as a badge and the side as a pip", async () => {
    m = await mount(UiPlayer, { seat: { player: "lichess:a", name: "Alpha", title: "BOT" }, colour: "black" });
    expect(text(m.el.querySelector(".ui-badge"))).toBe("BOT");
    expect(m.el.querySelector(".ui-player-pip--black")?.getAttribute("aria-label")).toBe("plays black");
  });

  it("links to a player's page, to yours and to an engine's", async () => {
    m = await mount(UiPlayer, { seat: { player: "lichess:alpha-bot", name: "Alpha-Bot" }, link: true });
    expect(m.el.querySelector("a")?.getAttribute("href")).toBe("/players/lichess:alpha-bot");
    m.props["seat"] = { name: "You", key: "you" };
    await m.flush();
    expect(m.el.querySelector("a")?.getAttribute("href")).toBe("/you");
    m.props["seat"] = { name: "Stockfish", key: "engine:stockfish" };
    await m.flush();
    expect(m.el.querySelector("a")?.getAttribute("href")).toBe("/players/engine:stockfish");
  });

  it("is not a link without a key, or without link", async () => {
    m = await mount(UiPlayer, { seat: { name: "Nobody" }, link: true });
    expect(m.el.querySelector("a")).toBeNull();
    m.props["seat"] = engineSeat;
    m.props["link"] = false;
    await m.flush();
    expect(m.el.querySelector("a")).toBeNull();
  });

  it("gives the whole name as a tooltip, since a long one is cut", async () => {
    m = await mount(UiPlayer, { seat: { name: "a-very-long-username-that-keeps-going" } });
    expect(m.el.querySelector(".ui-player")?.getAttribute("title")).toBe("a-very-long-username-that-keeps-going");
  });

  it("is a chip at size md, current when it is the page you are on", async () => {
    m = await mount(UiPlayer, { seat: { name: "You", key: "you" }, size: "md", link: true, current: true });
    const el = m.el.querySelector("a");
    expect(el?.className).toContain("ui-player--md");
    expect(el?.className).toContain("is-current");
    expect(el?.getAttribute("aria-current")).toBe("page");
  });

  it("shows the live dot before the name", async () => {
    m = await mount(UiPlayer, { seat: engineSeat, live: true });
    expect(m.el.querySelector('[aria-label="Live"]')).not.toBeNull();
  });
});

describe("UiLive", () => {
  it("is hidden from a screen reader unless it has a label", async () => {
    m = await mount(UiLive, {});
    expect(m.el.querySelector(".ui-live")?.getAttribute("aria-hidden")).toBe("true");
    m.props["label"] = "Live";
    await m.flush();
    expect(m.el.querySelector(".ui-live")?.getAttribute("role")).toBe("img");
  });

  it("pulses once each time the pulse value changes, never at first", async () => {
    m = await mount(UiLive, { pulse: 1 });
    expect(m.el.querySelector(".ui-live--pulse")).toBeNull();
    m.props["pulse"] = 2;
    await m.flush();
    expect(m.el.querySelector(".ui-live--pulse")).not.toBeNull();
  });

  it("has a tone", async () => {
    m = await mount(UiLive, { tone: "loss" });
    expect(m.el.querySelector(".ui-live--loss")).not.toBeNull();
  });
});

describe("UiStat", () => {
  it("writes numbers grouped and an en dash for nothing", async () => {
    m = await mount(UiStat, { label: "Games", value: 3708 });
    expect(text(m.el.querySelector(".ui-stat-value"))).toBe("3,708");
    m.props["value"] = null;
    await m.flush();
    expect(text(m.el.querySelector(".ui-stat-none"))).toBe("–");
    expect(text(m.el.querySelector(".sr-only"))).toBe("no value");
    m.props["value"] = "";
    await m.flush();
    expect(m.el.querySelector(".ui-stat-none")).not.toBeNull();
  });

  it("keeps a zero a zero and a decimal to one place unless told", async () => {
    m = await mount(UiStat, { label: "Won", value: 0 });
    expect(text(m.el.querySelector(".ui-stat-value"))).toBe("0");
    m.props["value"] = 62.43;
    await m.flush();
    expect(text(m.el.querySelector(".ui-stat-value"))).toBe("62.4");
    m.props["digits"] = 2;
    await m.flush();
    expect(text(m.el.querySelector(".ui-stat-value"))).toBe("62.43");
  });

  it("shows a unit only beside a value, and a delta in the outcome colours", async () => {
    m = await mount(UiStat, { label: "Score", value: 62, unit: "%", delta: 12 });
    expect(text(m.el.querySelector(".ui-stat-unit"))).toBe("%");
    expect(text(m.el.querySelector(".ui-stat-delta"))).toBe("+12");
    expect(m.el.querySelector(".ui-stat-delta--win")).not.toBeNull();
    m.props["delta"] = -3;
    await m.flush();
    expect(text(m.el.querySelector(".ui-stat-delta"))).toBe("−3");
    expect(m.el.querySelector(".ui-stat-delta--loss")).not.toBeNull();
    m.props["delta"] = "since May";
    await m.flush();
    expect(text(m.el.querySelector(".ui-stat-delta"))).toBe("since May");
    expect(m.el.querySelector(".ui-stat-delta--win, .ui-stat-delta--loss")).toBeNull();
    m.props["value"] = null;
    await m.flush();
    expect(m.el.querySelector(".ui-stat-unit")).toBeNull();
  });

  it("explains a glossary term in its label, and a plain hint as a tooltip", async () => {
    m = await mount(UiStat, { label: "Elo", value: 3, hint: "elo" });
    expect(m.el.querySelector(".e-toggletip-term, .e-toggletip")).not.toBeNull();
    m.props["hint"] = "Not a glossary key.";
    await m.flush();
    expect(m.el.querySelector(".ui-stat-label")?.getAttribute("title")).toBe("Not a glossary key.");
  });

  it("has a size and a line under", async () => {
    m = await mount(UiStat, { label: "Rating", value: 1840, size: "xl", sub: "± 12" });
    expect(m.el.querySelector(".ui-stat--xl")).not.toBeNull();
    expect(text(m.el.querySelector(".ui-stat-sub"))).toBe("± 12");
  });

  it("takes a custom value in its slot", async () => {
    m = await mount(UiStat, { label: "Custom" }, { default: () => h("b", "custom") });
    expect(text(m.el.querySelector(".ui-stat-value"))).toBe("custom");
  });
});

describe("UiStats", () => {
  it("fits as many as it can, or the count it is given", async () => {
    m = await mount(UiStats, {}, { default: () => h("span", "x") });
    expect(m.el.querySelector<HTMLElement>(".ui-stats")?.getAttribute("style")).toBeNull();
    m.props["columns"] = 3;
    await m.flush();
    expect(m.el.querySelector<HTMLElement>(".ui-stats")?.style.gridTemplateColumns).toContain("repeat(3");
    m.props["dense"] = true;
    await m.flush();
    expect(m.el.querySelector(".ui-stats--dense")).not.toBeNull();
  });
});

describe("UiSection", () => {
  it("titles the section with a heading and names the region by it", async () => {
    m = await mount(UiSection, { title: "Results" }, { default: () => h("p", "body") });
    const heading = m.el.querySelector("h2");
    expect(text(heading)).toBe("Results");
    expect(m.el.querySelector("section")?.getAttribute("aria-labelledby")).toBe(heading?.id);
    expect(text(m.el.querySelector(".ui-section-body"))).toBe("body");
  });

  it("is an h3 at level 2, or the level you give", async () => {
    m = await mount(UiSection, { title: "Part", level: 2 });
    expect(m.el.querySelector("h3")).not.toBeNull();
    m.props["headingLevel"] = 4;
    await m.flush();
    expect(m.el.querySelector("h4")).not.toBeNull();
  });

  it("has an aside and a note only when given", async () => {
    m = await mount(UiSection, { title: "T" });
    expect(m.el.querySelector(".ui-section-aside, .ui-section-note")).toBeNull();
    m.unmount();
    m = await mount(UiSection, { title: "T" }, { aside: () => "Filters", note: () => "A note." });
    expect(text(m.el.querySelector(".ui-section-aside"))).toBe("Filters");
    expect(text(m.el.querySelector(".ui-section-note"))).toBe("A note.");
  });

  it("collapses from its title, and says so", async () => {
    m = await mount(UiSection, { title: "More", collapsible: true }, { default: () => "inside" });
    const toggle = m.el.querySelector<HTMLButtonElement>(".ui-section-toggle");
    const body = m.el.querySelector<HTMLElement>(".ui-section-body");
    expect(toggle?.getAttribute("aria-expanded")).toBe("true");
    expect(toggle?.getAttribute("aria-controls")).toBe(body?.id);
    expect(body?.style.display).not.toBe("none");
    toggle?.click();
    await nextTick();
    expect(toggle?.getAttribute("aria-expanded")).toBe("false");
    expect(body?.style.display).toBe("none");
    toggle?.click();
    await nextTick();
    expect(toggle?.getAttribute("aria-expanded")).toBe("true");
  });

  it("is not a button when it cannot collapse", async () => {
    m = await mount(UiSection, { title: "Fixed" });
    expect(m.el.querySelector("button")).toBeNull();
  });
});

describe("UiEmpty", () => {
  it("says what is missing and what to do, with its actions", async () => {
    m = await mount(UiEmpty, { title: "No games yet", body: "Play one." }, { actions: () => h("button", "New game") });
    expect(text(m.el.querySelector(".ui-empty-title"))).toBe("No games yet");
    expect(text(m.el.querySelector(".ui-empty-body"))).toBe("Play one.");
    expect(text(m.el.querySelector(".ui-empty-actions"))).toBe("New game");
    expect(m.el.querySelector("[role=alert]")).toBeNull();
  });

  it("is an alert with an error icon when it is an error", async () => {
    m = await mount(UiEmpty, { title: "Could not load", tone: "error" });
    expect(m.el.querySelector("[role=alert]")).not.toBeNull();
    expect(m.el.querySelector(".ui-empty-icon.is-error")).not.toBeNull();
  });

  it("is one muted line when dense, with its actions inline", async () => {
    m = await mount(
      UiEmpty,
      { title: "No drills.", body: "Review a game.", dense: true },
      { actions: () => h("button", "Review") },
    );
    const line = m.el.querySelector(".ui-empty-dense");
    expect(text(line)).toBe("No drills. Review a game. Review");
    expect(m.el.querySelector(".ui-empty-icon")).toBeNull();
  });
});

describe("UiToolbar", () => {
  it("puts #end at the other edge, and is sticky when asked", async () => {
    m = await mount(UiToolbar, { sticky: true }, { default: () => "start", end: () => "end" });
    expect(text(m.el.querySelector(".ui-toolbar-end"))).toBe("end");
    expect(m.el.querySelector(".ui-toolbar--sticky")).not.toBeNull();
    expect(m.el.querySelector(".ui-toolbar--wrap")).not.toBeNull();
    m.props["wrap"] = false;
    m.props["align"] = "between";
    await m.flush();
    expect(m.el.querySelector(".ui-toolbar--wrap")).toBeNull();
    expect(m.el.querySelector(".ui-toolbar--between")).not.toBeNull();
  });

  it("is a named group when it has a label", async () => {
    m = await mount(UiToolbar, { label: "Game actions" });
    expect(m.el.querySelector("[role=group]")?.getAttribute("aria-label")).toBe("Game actions");
  });
});

describe("UiKeyValue", () => {
  it("lists labels and values from items, with a tone and mono", async () => {
    m = await mount(UiKeyValue, {
      items: [
        { label: "Cores", value: 15 },
        { label: "Path", value: "C:\\dameless", mono: true },
        { label: "State", value: "Healthy", tone: "win" },
      ],
    });
    const rows = [...m.el.querySelectorAll(".ui-kv")];
    expect(rows.map((r) => text(r.querySelector("dt")))).toEqual(["Cores", "Path", "State"]);
    expect(rows.map((r) => text(r.querySelector("dd")))).toEqual(["15", "C:\\dameless", "Healthy"]);
    expect(rows[1]?.querySelector(".mono")).not.toBeNull();
    expect(rows[2]?.querySelector(".ui-kv-text--win")).not.toBeNull();
  });

  it("takes rows as children and a slot per key", async () => {
    m = await mount(
      UiKeyValue,
      {},
      { default: () => [h(UiKv, { label: "One" }, () => "1"), h(UiKv, { label: "Two" }, () => "2")] },
    );
    expect([...m.el.querySelectorAll("dd")].map((d) => text(d))).toEqual(["1", "2"]);
    m.unmount();
    m = await mount(
      UiKeyValue,
      { items: [{ key: "x", label: "Custom", value: "plain" }] },
      { "value-x": () => h("b", "rich") },
    );
    expect(text(m.el.querySelector("dd"))).toBe("rich");
  });

  it("has a copy button only where asked, named for its row", async () => {
    m = await mount(UiKeyValue, {
      items: [
        { label: "Path", value: "x", copy: true },
        { label: "Plain", value: "y" },
      ],
    });
    const buttons = m.el.querySelectorAll("button");
    expect(buttons).toHaveLength(1);
    expect(buttons[0]?.getAttribute("aria-label")).toBe("Copy Path");
  });

  it("copies the value's text and says so", async () => {
    const written: string[] = [];
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: (s: string) => (written.push(s), Promise.resolve()) },
      configurable: true,
    });
    m = await mount(UiKeyValue, { items: [{ label: "Path", value: "C:\\x", copy: true }] });
    m.el.querySelector("button")?.click();
    await new Promise((r) => setTimeout(r, 0));
    await m.flush();
    expect(written).toEqual(["C:\\x"]);
    expect(m.el.querySelector("button")?.getAttribute("aria-label")).toBe("Copied Path");
  });

  it("is values only in one line when inline, with the labels kept for a screen reader", async () => {
    m = await mount(UiKeyValue, {
      inline: true,
      items: [
        { label: "Time control", value: "5+3" },
        { label: "Where", value: "local" },
      ],
    });
    expect(m.el.querySelectorAll("dt.sr-only")).toHaveLength(2);
    expect(m.el.querySelector(".ui-kvs--inline")).not.toBeNull();
    expect([...m.el.querySelectorAll("dd")].map((d) => text(d))).toEqual(["5+3", "local"]);
    expect([...m.el.querySelectorAll("dt")].map((d) => text(d))).toEqual(["Time control", "Where"]);
  });

  it("is two columns when asked, and dense", async () => {
    m = await mount(UiKeyValue, { items: [{ label: "A", value: 1 }], columns: 2, dense: true });
    expect(m.el.querySelector(".ui-kvs--2.ui-kvs--dense")).not.toBeNull();
  });
});

describe("UiField", () => {
  it("names its control with the label, and hands the id over", async () => {
    let seen = "";
    m = await mount(
      UiField,
      { label: "Title", hint: "Short." },
      {
        default: (s: { id: string; describedby?: string }) => (
          (seen = s.id),
          h("input", { id: s.id, "aria-describedby": s.describedby })
        ),
      },
    );
    const label = m.el.querySelector("label");
    expect(label?.getAttribute("for")).toBe(seen);
    expect(m.el.querySelector("input")?.id).toBe(seen);
    const note = m.el.querySelector(".ui-field-note");
    expect(text(note)).toBe("Short.");
    expect(m.el.querySelector("input")?.getAttribute("aria-describedby")).toBe(note?.id);
  });

  it("uses an id you give, and says when it is optional", async () => {
    m = await mount(UiField, { label: "Branch", for: "branch", optional: true });
    expect(m.el.querySelector("label")?.getAttribute("for")).toBe("branch");
    expect(text(m.el.querySelector(".ui-field-optional"))).toBe("Optional");
  });

  it("shows the error in the hint's place, announced, and marks the control invalid", async () => {
    let invalid: boolean | undefined;
    m = await mount(
      UiField,
      { label: "Title", hint: "Short.", error: "Give it a title." },
      { default: (s: { invalid: boolean }) => ((invalid = s.invalid), "") },
    );
    const notes = m.el.querySelectorAll(".ui-field-note");
    expect(notes).toHaveLength(1);
    expect(text(notes[0])).toBe("Give it a title.");
    expect(notes[0]?.getAttribute("aria-live")).toBe("polite");
    expect(notes[0]?.className).toContain("is-error");
    expect(invalid).toBe(true);
  });

  it("labels a group of controls as a group instead of pointing at one input", async () => {
    m = await mount(UiField, { label: "Time control", group: true });
    expect(m.el.querySelector("label")).toBeNull();
    const group = m.el.querySelector("[role=group]");
    expect(group?.getAttribute("aria-labelledby")).toBe(m.el.querySelector(".ui-field-label")?.id);
  });
});

describe("UiPanelHead", () => {
  it("draws nothing without a game", async () => {
    m = await mount(UiPanelHead, { game: null, mode: "explore" });
    expect(m.el.querySelector(".ui-head")).toBeNull();
  });

  it("names the players with the result between them", async () => {
    m = await mount(UiPanelHead, { game: wonGame, mode: "review" });
    const names = [...m.el.querySelectorAll(".ui-player-name")].map((n) => text(n));
    expect(names).toEqual(["Alpha-Bot", "You"]);
    expect(text(m.el.querySelector(".ui-result-mark"))).toBe("0-1");
    expect(m.el.querySelector(".ui-result--win")).not.toBeNull();
  });

  it("states opening, clock, rating and where on one muted line", async () => {
    m = await mount(UiPanelHead, { game: wonGame, mode: "review" });
    expect(text(m.el.querySelector(".ui-kvs--inline"))).toContain("Sicilian Defense: Najdorf Variation");
    expect(text(m.el.querySelector(".ui-kvs--inline"))).toContain("5+3");
    expect(text(m.el.querySelector(".ui-kvs--inline"))).toContain("rated");
    expect(text(m.el.querySelector(".ui-kvs--inline"))).toContain("Lichess");
  });

  it("says a local game is local and sets the two names of a live one against each other", async () => {
    m = await mount(UiPanelHead, { game: liveGame, mode: "play" });
    expect(text(m.el.querySelector(".ui-kvs--inline"))).toContain("local");
    expect(text(m.el.querySelector(".ui-head-vs"))).toBe("vs");
    expect(m.el.querySelector(".ui-result-mark")).toBeNull();
  });

  it("says how a finished game ended when nothing else says what is happening", async () => {
    m = await mount(UiPanelHead, { game: strangersGame, mode: "watch" });
    expect(text(m.el.querySelector(".ui-head-status"))).toBe("Draw by repetition");
    expect(text(m.el.querySelector(".ui-result-mark"))).toBe("½-½");
    expect(m.el.querySelector(".ui-result--win, .ui-result--loss")).toBeNull();
  });

  it("shows the status and the actions it is given", async () => {
    m = await mount(
      UiPanelHead,
      { game: liveGame, mode: "play" },
      { status: () => "Your move", actions: () => h("button", "Resign") },
    );
    expect(text(m.el.querySelector(".ui-head-status"))).toBe("Your move");
    expect(text(m.el.querySelector(".ui-head-actions"))).toBe("Resign");
  });
});
