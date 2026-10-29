import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createApp, type App } from "vue";
import { createPinia, setActivePinia } from "pinia";
import ReviewBar from "../src/parts/ReviewBar.vue";
import { coverage, windowName } from "../src/format";

vi.mock("@eos-ui/vue", async (orig) => ({
  ...(await orig<typeof import("@eos-ui/vue")>()),
  useToast: () => ({ show: () => {} }),
}));

let app: App | null = null;
let host: HTMLElement | null = null;

function mount(props: Record<string, unknown>): string {
  const pinia = createPinia();
  setActivePinia(pinia);
  host = document.createElement("div");
  document.body.append(host);
  app = createApp(ReviewBar, props).use(pinia);
  app.mount(host);
  return host.textContent?.replace(/\s+/g, " ") ?? "";
}

beforeEach(() => {
  document.body.innerHTML = "";
});
afterEach(() => {
  app?.unmount();
  host?.remove();
  app = null;
  host = null;
});

describe("the review bar", () => {
  it("counts every game not reviewed, not the profile's window of 200", () => {
    // 3,708 imported games, none reviewed; the profile covers the newest 200.
    const text = mount({ playerKey: "you", name: "You", mine: true, games: 200, pending: 200, total: 3708 });
    expect(text).toContain("3,708 games not reviewed yet");
    expect(text).toContain("Review all 3,708");
    expect(text).not.toContain("200 games not reviewed");
  });

  it("falls back to the window's count while the total is unknown", () => {
    const text = mount({ playerKey: "you", name: "You", mine: true, games: 200, pending: 12, total: null });
    expect(text).toContain("12 games not reviewed yet");
  });

  it("offers nothing to review when every game is reviewed, though the window has none pending", () => {
    const text = mount({ playerKey: "you", name: "You", mine: true, games: 200, pending: 0, total: 0 });
    expect(text).not.toContain("not reviewed");
  });
});

describe("the games window", () => {
  it("names the choices and says what the figures cover", () => {
    expect(windowName(200)).toBe("Last 200 games");
    expect(windowName(1_000_000)).toBe("All games");
    expect(coverage(200, 200, "your")).toBe("your last 200 games");
    expect(coverage(1_000_000, 3708)).toBe("all 3,708 games");
    expect(coverage(500, 64, "your")).toBe("all 64 games");
  });
});
