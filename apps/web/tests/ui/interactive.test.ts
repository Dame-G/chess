import { h, nextTick } from "vue";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { answer, current, registerHost } from "../../src/ui/useConfirm";
import { useConfirm } from "../../src/ui/useConfirm";
import UiSeg from "../../src/ui/UiSeg.vue";
import UiSheetPage from "../../src/ui/UiSheetPage.vue";
import UiTable from "../../src/ui/UiTable.vue";
import { mount, key, text, type Mounted } from "./mount";

let m: Mounted | undefined;
afterEach(() => {
  m?.unmount();
  m = undefined;
});

describe("UiSeg", () => {
  const options = [
    { value: "a", label: "Alpha" },
    { value: "b", label: "Beta", count: 12 },
    { value: "c", label: "Gamma", disabled: true },
    { value: "d", label: "Delta" },
  ];

  /** Mounts a one-choice control whose model follows what it emits, as `v-model` would. */
  async function single(extra: Record<string, unknown> = {}) {
    const mounted = await mount(UiSeg, {
      modelValue: "a",
      options,
      label: "Letter",
      "onUpdate:modelValue": (v: string) => (mounted.props["modelValue"] = v),
      ...extra,
    });
    return mounted;
  }
  const radios = (el: HTMLElement) => [...el.querySelectorAll<HTMLButtonElement>("[role=radio]")];

  it("is a named radio group, one option checked, one in the tab order", async () => {
    m = await single();
    const group = m.el.querySelector("[role=radiogroup]");
    expect(group?.getAttribute("aria-label")).toBe("Letter");
    expect(radios(m.el).map((r) => r.getAttribute("aria-checked"))).toEqual(["true", "false", "false", "false"]);
    expect(radios(m.el).map((r) => r.tabIndex)).toEqual([0, -1, -1, -1]);
  });

  it("shows a count after the label", async () => {
    m = await single();
    expect(text(radios(m.el)[1])).toBe("Beta12");
  });

  it("chooses on click and moves the tab stop with it", async () => {
    m = await single();
    radios(m.el)[3]?.click();
    await m.flush();
    expect(m.props["modelValue"]).toBe("d");
    expect(radios(m.el).map((r) => r.tabIndex)).toEqual([-1, -1, -1, 0]);
  });

  it("does not emit when the chosen option is clicked again, or a disabled one", async () => {
    let emitted = 0;
    m = await single({ "onUpdate:modelValue": () => emitted++ });
    radios(m.el)[0]?.click();
    radios(m.el)[2]?.click();
    expect(emitted).toBe(0);
  });

  it("moves with the arrow keys, skipping a disabled option and wrapping", async () => {
    m = await single();
    const first = radios(m.el)[0]!;
    key(first, "ArrowRight");
    await m.flush();
    expect(m.props["modelValue"]).toBe("b");
    key(radios(m.el)[1]!, "ArrowRight");
    await m.flush();
    expect(m.props["modelValue"]).toBe("d");
    key(radios(m.el)[3]!, "ArrowRight");
    await m.flush();
    expect(m.props["modelValue"]).toBe("a");
    key(radios(m.el)[0]!, "ArrowLeft");
    await m.flush();
    expect(m.props["modelValue"]).toBe("d");
    key(radios(m.el)[3]!, "ArrowUp");
    await m.flush();
    expect(m.props["modelValue"]).toBe("b");
  });

  it("jumps to the ends with Home and End", async () => {
    m = await single({ modelValue: "b" });
    key(radios(m.el)[1]!, "End");
    await m.flush();
    expect(m.props["modelValue"]).toBe("d");
    key(radios(m.el)[3]!, "Home");
    await m.flush();
    expect(m.props["modelValue"]).toBe("a");
  });

  it("puts the first enabled option in the tab order when nothing is chosen", async () => {
    m = await single({ modelValue: undefined });
    expect(radios(m.el).map((r) => r.tabIndex)).toEqual([0, -1, -1, -1]);
  });

  it("is a group of toggles when several can be on", async () => {
    const mounted = await mount(UiSeg, {
      values: ["a"],
      options,
      label: "Letters",
      multiple: true,
      "onUpdate:values": (v: string[]) => (mounted.props["values"] = v),
    });
    m = mounted;
    expect(m.el.querySelector("[role=group]")?.getAttribute("aria-label")).toBe("Letters");
    const buttons = [...m.el.querySelectorAll("button")];
    expect(buttons.map((b) => b.getAttribute("aria-pressed"))).toEqual(["true", "false", "false", "false"]);
    expect(buttons.every((b) => b.tabIndex === 0)).toBe(true);
    buttons[1]?.click();
    await m.flush();
    expect(m.props["values"]).toEqual(["a", "b"]);
    buttons[0]?.click();
    await m.flush();
    expect(m.props["values"]).toEqual(["b"]);
  });

  it("has a size and can fill the width", async () => {
    m = await single({ size: "sm", full: true });
    expect(m.el.querySelector(".ui-seg--sm.ui-seg--full")).not.toBeNull();
  });

  it("shows an icon before the label", async () => {
    m = await single({
      options: [{ value: "a", label: "Alpha", icon: { render: () => h("svg", { class: "icon" }) } }],
    });
    expect(m.el.querySelector("button .icon")).not.toBeNull();
  });
});

describe("UiTable", () => {
  const columns = [
    { key: "name", label: "Level" },
    { key: "elo", label: "Rating", fig: true, sort: true },
    { key: "nodes", label: "Nodes", fig: true, hide: "sm" as const },
  ];
  const rows = [
    { id: "1", name: "Level 1", elo: 820, nodes: 300 },
    { id: "2", name: "Level 2", elo: 1105, nodes: null },
    { id: "3", name: "Level 3", elo: 1390, nodes: 5000 },
  ];
  const body = (el: HTMLElement) => [...el.querySelectorAll("tbody tr")];
  const cells = (tr: Element | undefined) => [...(tr?.querySelectorAll("td") ?? [])].map((td) => text(td));

  it("is a named table with column headers", async () => {
    m = await mount(UiTable, { columns, rows, rowKey: "id", label: "Levels" });
    expect(m.el.querySelector("table")?.getAttribute("aria-label")).toBe("Levels");
    expect([...m.el.querySelectorAll("th")].map((th) => th.getAttribute("scope"))).toEqual(["col", "col", "col"]);
    expect([...m.el.querySelectorAll("th")].map((th) => text(th))).toEqual(["Level", "Rating", "Nodes"]);
  });

  it("writes figures grouped and aligned to the end, and nothing as an en dash", async () => {
    m = await mount(UiTable, { columns, rows, rowKey: "id" });
    expect(cells(body(m.el)[0])).toEqual(["Level 1", "820", "300"]);
    expect(cells(body(m.el)[1])).toEqual(["Level 2", "1,105", "–"]);
    expect(cells(body(m.el)[2])).toEqual(["Level 3", "1,390", "5,000"]);
    const tds = [...body(m.el)[0]!.querySelectorAll("td")];
    expect(tds.map((td) => td.className)).toEqual([
      expect.stringContaining("is-start"),
      expect.stringContaining("is-end"),
      expect.stringContaining("is-end"),
    ]);
    expect(tds[0]?.className).toContain("is-first");
    expect(tds[1]?.className).toContain("is-fig");
    expect(tds[0]?.className).not.toContain("is-fig");
  });

  it("takes a row key by field or by function", async () => {
    m = await mount(UiTable, { columns, rows, rowKey: (r: { id: string }) => `row-${r.id}`, selected: "row-2" });
    expect(body(m.el).map((tr) => tr.getAttribute("aria-current"))).toEqual([null, "true", null]);
    expect(body(m.el)[1]?.className).toContain("is-selected");
  });

  it("renders a cell slot with the row and its value", async () => {
    m = await mount(
      UiTable,
      { columns, rows, rowKey: "id" },
      { "cell-name": (s: { row: { name: string }; value: unknown }) => h("b", `${s.row.name}:${String(s.value)}`) },
    );
    expect(text(body(m.el)[0]?.querySelector("b"))).toBe("Level 1:Level 1");
  });

  it("sorts on a header click: a figure column starts with the biggest, then flips", async () => {
    const sorted: [string, string][] = [];
    m = await mount(UiTable, { columns, rows, rowKey: "id", onSort: (k: string, d: string) => sorted.push([k, d]) });
    const button = m.el.querySelector<HTMLButtonElement>("th button")!;
    button.click();
    await m.flush();
    expect(sorted).toEqual([["elo", "desc"]]);
    expect(body(m.el).map((tr) => cells(tr)[0])).toEqual(["Level 3", "Level 2", "Level 1"]);
    expect(m.el.querySelectorAll("th")[1]?.getAttribute("aria-sort")).toBe("descending");
    button.click();
    await m.flush();
    expect(sorted[1]).toEqual(["elo", "asc"]);
    expect(body(m.el).map((tr) => cells(tr)[0])).toEqual(["Level 1", "Level 2", "Level 3"]);
    expect(m.el.querySelectorAll("th")[1]?.getAttribute("aria-sort")).toBe("ascending");
    expect(m.el.querySelectorAll("th")[0]?.getAttribute("aria-sort")).toBeNull();
  });

  it("leaves the order to the parent when manual, and shows the parent's arrow", async () => {
    m = await mount(UiTable, { columns, rows, rowKey: "id", manual: true, sort: { key: "elo", dir: "asc" } });
    m.el.querySelector<HTMLButtonElement>("th button")!.click();
    await m.flush();
    expect(body(m.el).map((tr) => cells(tr)[0])).toEqual(["Level 1", "Level 2", "Level 3"]);
    expect(m.el.querySelectorAll("th")[1]?.getAttribute("aria-sort")).toBe("ascending");
  });

  it("makes its rows buttons only when something listens for a selection", async () => {
    m = await mount(UiTable, { columns, rows, rowKey: "id" });
    expect(body(m.el)[0]?.getAttribute("tabindex")).toBeNull();
    m.unmount();
    const chosen: string[] = [];
    m = await mount(UiTable, { columns, rows, rowKey: "id", onSelect: (r: { id: string }) => chosen.push(r.id) });
    const first = body(m.el)[0] as HTMLElement;
    expect(first.getAttribute("tabindex")).toBe("0");
    first.click();
    key(first, "Enter");
    key(first, " ");
    expect(chosen).toEqual(["1", "1", "1"]);
  });

  it("does not select from a key pressed inside a cell", async () => {
    const chosen: string[] = [];
    m = await mount(
      UiTable,
      {
        columns,
        rows,
        rowKey: "id",
        onSelect: (r: { id: string }) => chosen.push(r.id),
      },
      { "cell-name": () => h("input", { class: "inside" }) },
    );
    key(m.el.querySelector(".inside")!, "Enter");
    expect(chosen).toEqual([]);
  });

  it("shows skeleton rows while it loads, and says it is busy", async () => {
    m = await mount(UiTable, { columns, rows: [], rowKey: "id", loading: true });
    expect(m.el.querySelectorAll(".ui-table-skeleton")).toHaveLength(5);
    expect(m.el.querySelector("table")?.getAttribute("aria-busy")).toBe("true");
    expect(m.el.querySelector(".ui-table-none")).toBeNull();
  });

  it("says what is missing when it is empty", async () => {
    m = await mount(UiTable, { columns, rows: [], rowKey: "id", empty: "No levels yet." });
    expect(text(m.el.querySelector(".ui-table-none"))).toBe("No levels yet.");
    expect(m.el.querySelector(".ui-table-none td")?.getAttribute("colspan")).toBe("3");
    m.props["empty"] = undefined;
    await m.flush();
    expect(text(m.el.querySelector(".ui-table-none"))).toBe("Nothing to show.");
  });

  it("marks a column to hide below a breakpoint on its head and every cell", async () => {
    m = await mount(UiTable, { columns, rows, rowKey: "id" });
    expect(m.el.querySelectorAll('[data-hide="sm"]')).toHaveLength(1 + rows.length);
  });

  it("shows a row's aside only while it is expanded, and a footer when it has one", async () => {
    m = await mount(
      UiTable,
      { columns, rows, rowKey: "id", expanded: ["2"] },
      { "row-aside": (s: { row: { name: string } }) => `more on ${s.row.name}`, foot: () => "3 levels" },
    );
    const asides = m.el.querySelectorAll(".ui-table-aside");
    expect(asides).toHaveLength(1);
    expect(text(asides[0])).toBe("more on Level 2");
    expect(text(m.el.querySelector("tfoot"))).toBe("3 levels");
    m.props["expanded"] = [];
    await m.flush();
    expect(m.el.querySelector(".ui-table-aside")).toBeNull();
  });

  it("takes a head slot", async () => {
    m = await mount(UiTable, { columns, rows, rowKey: "id" }, { "head-name": () => h("em", "Which level") });
    expect(text(m.el.querySelector("th em"))).toBe("Which level");
  });
});

describe("UiSheetPage", () => {
  const tabs = [
    { id: "overview", label: "Overview" },
    { id: "games", label: "Games", count: 3708 },
    { id: "drills", label: "Drills" },
  ];

  async function sheet(extra: Record<string, unknown> = {}, slots: Parameters<typeof mount>[2] = {}) {
    const mounted = await mount(
      UiSheetPage,
      {
        title: "You",
        subtitle: "dame_g · Lichess",
        tabs,
        contained: true,
        sync: false,
        tab: "overview",
        "onUpdate:tab": (t: string) => (mounted.props["tab"] = t),
        onClose: () => {},
        ...extra,
      },
      { default: (s: { tab: string }) => `content of ${s.tab}`, ...slots },
    );
    return mounted;
  }

  it("is a dialog named by its title, which is an h1 that takes focus and shows no box", async () => {
    m = await sheet();
    await nextTick();
    const dialog = m.el.querySelector("[role=dialog]");
    const heading = m.el.querySelector("h1");
    expect(text(heading)).toBe("You");
    expect(dialog?.getAttribute("aria-labelledby")).toBe(heading?.id);
    expect(heading?.getAttribute("tabindex")).toBe("-1");
    expect(text(m.el.querySelector(".ui-sheet-subtitle"))).toBe("dame_g · Lichess");
    await new Promise((r) => setTimeout(r, 0));
    expect(document.activeElement).toBe(heading);
  });

  it("shows tabs as a tablist, the chosen one selected, wired to its panel", async () => {
    m = await sheet();
    const buttons = [...m.el.querySelectorAll<HTMLButtonElement>("[role=tab]")];
    expect(buttons.map((b) => text(b))).toEqual(["Overview", "Games3708", "Drills"]);
    expect(buttons.map((b) => b.getAttribute("aria-selected"))).toEqual(["true", "false", "false"]);
    expect(buttons.map((b) => b.tabIndex)).toEqual([0, -1, -1]);
    const panel = m.el.querySelector("[role=tabpanel]");
    expect(panel?.id).toBe(buttons[0]?.getAttribute("aria-controls"));
    expect(panel?.getAttribute("aria-labelledby")).toBe(buttons[0]?.id);
    expect(text(panel)).toBe("content of overview");
  });

  it("switches tabs on click and on the arrow keys, and wraps", async () => {
    m = await sheet();
    const buttons = () => [...m!.el.querySelectorAll<HTMLButtonElement>("[role=tab]")];
    buttons()[1]?.click();
    await m.flush();
    expect(m.props["tab"]).toBe("games");
    expect(text(m.el.querySelector("[role=tabpanel]"))).toBe("content of games");
    key(buttons()[1]!, "ArrowRight");
    await m.flush();
    expect(m.props["tab"]).toBe("drills");
    key(buttons()[2]!, "ArrowRight");
    await m.flush();
    expect(m.props["tab"]).toBe("overview");
    key(buttons()[0]!, "ArrowLeft");
    await m.flush();
    expect(m.props["tab"]).toBe("drills");
    key(buttons()[2]!, "Home");
    await m.flush();
    expect(m.props["tab"]).toBe("overview");
    key(buttons()[0]!, "End");
    await m.flush();
    expect(m.props["tab"]).toBe("drills");
  });

  it("uses a tab's own slot when it has one", async () => {
    m = await sheet({}, { "tab-games": () => "the games tab" });
    m.props["tab"] = "games";
    await m.flush();
    expect(text(m.el.querySelector("[role=tabpanel]"))).toBe("the games tab");
    m.props["tab"] = "drills";
    await m.flush();
    expect(text(m.el.querySelector("[role=tabpanel]"))).toBe("content of drills");
  });

  it("falls back to the first tab when the one asked for is not there", async () => {
    m = await sheet({ tab: "nowhere" });
    expect(text(m.el.querySelector("[role=tabpanel]"))).toBe("content of overview");
  });

  it("hides a tab row of one tab", async () => {
    m = await sheet({ tabs: [{ id: "only", label: "Only" }], tab: "only" });
    expect(m.el.querySelector("[role=tablist]")).toBeNull();
    expect(m.el.querySelector("[role=tabpanel]")).toBeNull();
    expect(text(m.el.querySelector(".ui-sheet-body"))).toBe("content of only");
  });

  it("has a hero strip and header actions where given", async () => {
    m = await sheet({}, { lead: () => "hero", actions: () => h("button", "Review all") });
    expect(text(m.el.querySelector(".ui-sheet-lead"))).toBe("hero");
    expect(text(m.el.querySelector(".ui-sheet-actions"))).toBe("Review all");
  });

  it("closes with its button and with Escape, through the listener when there is one", async () => {
    let closed = 0;
    m = await sheet({ onClose: () => closed++ });
    m.el.querySelector<HTMLButtonElement>('[aria-label="Close"]')?.click();
    expect(closed).toBe(1);
    key(document.body, "Escape");
    expect(closed).toBe(2);
  });

  it("leaves Escape to a dialog open over it", async () => {
    let closed = 0;
    m = await sheet({ onClose: () => closed++ });
    const layer = document.createElement("div");
    layer.setAttribute("role", "alertdialog");
    document.body.append(layer);
    key(document.body, "Escape");
    layer.remove();
    expect(closed).toBe(0);
  });

  it("shows a skeleton of its final shape while it loads", async () => {
    m = await sheet({ loading: true });
    expect(m.el.querySelectorAll(".skel").length).toBeGreaterThan(3);
    expect(m.el.querySelector("[role=tablist]")).toBeNull();
    expect(m.el.querySelector("[role=dialog]")?.getAttribute("aria-busy")).toBe("true");
    expect(m.el.querySelector("h1")).not.toBeNull();
  });

  it("says what failed, in the same frame, and offers to try again", async () => {
    let retried = 0;
    m = await sheet({ error: "The app did not answer.", onRetry: () => retried++ });
    expect(m.el.querySelector("[role=alert]")).not.toBeNull();
    expect(text(m.el.querySelector(".ui-empty-body"))).toBe("The app did not answer.");
    expect(m.el.querySelector("h1")).not.toBeNull();
    [...m.el.querySelectorAll("button")].find((b) => text(b) === "Try again")?.click();
    expect(retried).toBe(1);
  });

  it("keeps the chosen tab in the address", async () => {
    m = await sheet({ sync: true });
    await m.router.push("/you");
    m.el.querySelectorAll<HTMLButtonElement>("[role=tab]")[2]?.click();
    await m.flush();
    await new Promise((r) => setTimeout(r, 0));
    expect(m.router.currentRoute.value.query["tab"]).toBe("drills");
  });

  it("opens on the tab the address names", async () => {
    let chosen = "";
    m = await mount(
      UiSheetPage,
      { title: "You", tabs, contained: true, onClose: () => {}, "onUpdate:tab": (t: string) => (chosen = t) },
      { default: (s: { tab: string }) => `content of ${s.tab}` },
      "/you?tab=games",
    );
    expect(chosen).toBe("games");
  });
});

describe("useConfirm", () => {
  beforeEach(() => {
    while (current.value) answer(false);
  });

  it("refuses loudly when no host is mounted, instead of hanging", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    expect(await useConfirm()({ title: "Resign?", confirmLabel: "Resign" })).toBe(false);
    expect(warn).toHaveBeenCalled();
    warn.mockRestore();
  });

  it("resolves true for the action and false for a cancel", async () => {
    const off = registerHost();
    const confirm = useConfirm();
    const first = confirm({ title: "Resign this game?", confirmLabel: "Resign", danger: true });
    expect(current.value?.title).toBe("Resign this game?");
    expect(current.value?.danger).toBe(true);
    answer(true);
    expect(await first).toBe(true);
    expect(current.value).toBeNull();
    const second = confirm({ title: "Push?", confirmLabel: "Push" });
    answer(false);
    expect(await second).toBe(false);
    off();
  });

  it("asks one at a time, in order", async () => {
    const off = registerHost();
    const confirm = useConfirm();
    const a = confirm({ title: "A?", confirmLabel: "Yes" });
    const b = confirm({ title: "B?", confirmLabel: "Yes" });
    expect(current.value?.title).toBe("A?");
    answer(true);
    expect(current.value?.title).toBe("B?");
    answer(false);
    expect(await a).toBe(true);
    expect(await b).toBe(false);
    expect(current.value).toBeNull();
    off();
  });

  it("ignores an answer when nothing is asked", () => {
    expect(() => answer(true)).not.toThrow();
  });
});
