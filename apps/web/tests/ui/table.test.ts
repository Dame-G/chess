import { describe, expect, it } from "vitest";
import { cellText, columnAlign, compareValues, nextSort, sortRows, type TableColumn } from "../../src/ui/table";
import { confirmsOnEnter } from "../../src/ui/dialog";
import { BREAKPOINTS, down, up } from "../../src/media";
import { contrastRatio } from "../../src/ui/kit/contrast";

const col = (patch: Partial<TableColumn> = {}): TableColumn => ({ key: "k", label: "K", ...patch });

describe("table columns", () => {
  it("aligns figures to the end and words to the start unless told otherwise", () => {
    expect(columnAlign(col())).toBe("start");
    expect(columnAlign(col({ fig: true }))).toBe("end");
    expect(columnAlign(col({ fig: true, align: "center" }))).toBe("center");
    expect(columnAlign(col({ align: "end" }))).toBe("end");
  });

  it("writes cells: grouped figures, nothing as an en dash", () => {
    expect(cellText(3708)).toBe("3,708");
    expect(cellText(62.43)).toBe("62.4");
    expect(cellText("Italian Game")).toBe("Italian Game");
    expect(cellText(null)).toBe("–");
    expect(cellText(undefined)).toBe("–");
    expect(cellText("")).toBe("–");
    expect(cellText(0)).toBe("0");
  });
});

describe("table sorting", () => {
  it("orders numbers by size and text by locale with digits in order", () => {
    expect(compareValues(2, 10, "asc")).toBeLessThan(0);
    expect(compareValues(2, 10, "desc")).toBeGreaterThan(0);
    expect(compareValues("Level 2", "Level 10", "asc")).toBeLessThan(0);
    expect(compareValues("alpha", "Beta", "asc")).toBeLessThan(0);
    expect(compareValues(5, 5, "asc")).toBe(0);
  });

  it("puts nothing last whichever way it runs", () => {
    expect(compareValues(null, 1, "asc")).toBeGreaterThan(0);
    expect(compareValues(null, 1, "desc")).toBeGreaterThan(0);
    expect(compareValues(1, undefined, "desc")).toBeLessThan(0);
    expect(compareValues(null, undefined, "asc")).toBe(0);
  });

  it("sorts a copy, and leaves the order alone with nothing chosen", () => {
    const rows = [{ n: 3 }, { n: 1 }, { n: null }, { n: 2 }];
    expect(sortRows(rows, { key: "n", dir: "asc" }).map((r) => r.n)).toEqual([1, 2, 3, null]);
    expect(sortRows(rows, { key: "n", dir: "desc" }).map((r) => r.n)).toEqual([3, 2, 1, null]);
    expect(sortRows(rows, null).map((r) => r.n)).toEqual([3, 1, null, 2]);
    expect(rows.map((r) => r.n)).toEqual([3, 1, null, 2]);
  });

  it("is stable for equal keys", () => {
    const rows = [
      { n: 1, id: "a" },
      { n: 1, id: "b" },
      { n: 0, id: "c" },
    ];
    expect(sortRows(rows, { key: "n", dir: "asc" }).map((r) => r.id)).toEqual(["c", "a", "b"]);
  });

  it("starts figures with the biggest and words with the first, then flips, and never clears", () => {
    expect(nextSort(null, col({ key: "name" }))).toEqual({ key: "name", dir: "asc" });
    expect(nextSort(null, col({ key: "elo", fig: true }))).toEqual({ key: "elo", dir: "desc" });
    expect(nextSort({ key: "elo", dir: "desc" }, col({ key: "elo", fig: true }))).toEqual({ key: "elo", dir: "asc" });
    expect(nextSort({ key: "elo", dir: "asc" }, col({ key: "elo", fig: true }))).toEqual({ key: "elo", dir: "desc" });
    expect(nextSort({ key: "elo", dir: "asc" }, col({ key: "name" }))).toEqual({ key: "name", dir: "asc" });
  });
});

describe("what keeps Enter in a dialog", () => {
  const press = (target: Element, init: KeyboardEventInit = {}) => {
    const e = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true, ...init });
    Object.defineProperty(e, "target", { value: target });
    return confirmsOnEnter(e);
  };
  const make = (html: string) => {
    const host = document.createElement("div");
    host.innerHTML = html;
    return host.firstElementChild as Element;
  };

  it("confirms from a text field", () => {
    expect(press(make("<input />"))).toBe(true);
  });

  it("does not confirm from a button, a link or a combobox, which use Enter themselves", () => {
    expect(press(make("<button>Cancel</button>"))).toBe(false);
    expect(press(make('<a href="/x">x</a>'))).toBe(false);
    expect(press(make('<input role="combobox" />'))).toBe(false);
  });

  it("keeps plain Enter in a textarea and takes Ctrl or Cmd+Enter", () => {
    const area = make("<textarea></textarea>");
    expect(press(area)).toBe(false);
    expect(press(area, { ctrlKey: true })).toBe(true);
    expect(press(area, { metaKey: true })).toBe(true);
  });

  it("ignores other keys, other modifiers, composing text and a press already handled", () => {
    const input = make("<input />");
    expect(press(input, { shiftKey: true })).toBe(false);
    expect(press(input, { altKey: true })).toBe(false);
    expect(press(input, { ctrlKey: true })).toBe(false);
    expect(press(input, { isComposing: true })).toBe(false);
    const other = new KeyboardEvent("keydown", { key: "a", bubbles: true });
    Object.defineProperty(other, "target", { value: input });
    expect(confirmsOnEnter(other)).toBe(false);
  });
});

describe("breakpoints and contrast", () => {
  it("names one list of breakpoints", () => {
    expect(BREAKPOINTS).toEqual({ sm: "30rem", md: "48rem", lg: "75rem", xl: "100rem" });
    expect(up("lg")).toBe("(width >= 75rem)");
    expect(down("md")).toBe("(width < 48rem)");
  });

  it("measures contrast as WCAG does", () => {
    expect(contrastRatio([0, 0, 0], [255, 255, 255])).toBeCloseTo(21, 5);
    expect(contrastRatio([255, 255, 255], [255, 255, 255])).toBeCloseTo(1, 5);
    expect(contrastRatio([119, 119, 119], [255, 255, 255])).toBeCloseTo(4.48, 1);
    expect(contrastRatio([255, 255, 255], [0, 0, 0])).toBeCloseTo(21, 5);
  });
});
