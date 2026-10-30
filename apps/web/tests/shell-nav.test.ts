import { afterEach, describe, expect, it } from "vitest";
import { nextTick } from "vue";
import { useUi } from "../src/shell/ui";
import TabBar from "../src/shell/TabBar.vue";
import { mount, text, type Mounted } from "./ui/mount";

let m: Mounted | undefined;
afterEach(() => {
  m?.unmount();
  m = undefined;
});

const tabs = (el: HTMLElement) => [...el.querySelectorAll<HTMLButtonElement>(".tab")];

describe("the phone's tab bar", () => {
  it("has the three places, the board current at the root", async () => {
    m = await mount(TabBar);
    expect(tabs(m.el).map((t) => text(t))).toEqual(["Board", "Games", "You"]);
    expect(tabs(m.el).map((t) => t.getAttribute("aria-current"))).toEqual(["page", null, null]);
    expect(m.el.querySelector("nav")?.getAttribute("aria-label")).toBe("Main");
  });

  it("opens the Games rail over the board, and the board closes it", async () => {
    m = await mount(TabBar);
    const ui = useUi();
    tabs(m.el)[1]!.click();
    await nextTick();
    expect(ui.railOpen).toBe(true);
    expect(tabs(m.el).map((t) => t.getAttribute("aria-current"))).toEqual([null, "page", null]);
    tabs(m.el)[0]!.click();
    await nextTick();
    expect(ui.railOpen).toBe(false);
  });

  it("goes to a sheet and marks it current; the rail steps aside", async () => {
    m = await mount(TabBar);
    const ui = useUi();
    ui.railOpen = true;
    tabs(m.el)[2]!.click();
    await m.router.isReady();
    await new Promise((r) => setTimeout(r, 0));
    await nextTick();
    expect(m.router.currentRoute.value.path).toBe("/you");
    expect(ui.railOpen).toBe(false);
    expect(tabs(m.el).map((t) => t.getAttribute("aria-current"))).toEqual([null, null, "page"]);
  });
});
