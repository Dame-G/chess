import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createApp, nextTick, type App } from "vue";
import { createPinia } from "pinia";
import { PiniaColada } from "@pinia/colada";
import { createEos } from "@eos-ui/vue/plugin";
import "@eos-ui/vue/icons/builtin";
import SeekPanel from "../src/components/lichess/SeekPanel.vue";
import { clockOptions, parseClock, type LichessStatus } from "../src/lichess";

const load = (name: string) =>
  JSON.parse(readFileSync(join(process.cwd(), "tests", "fixtures", name), "utf8")) as LichessStatus;

let app: App | undefined;
afterEach(() => {
  app?.unmount();
  app = undefined;
  vi.unstubAllGlobals();
});

/** Mounts the panel and records the calls it makes to the runner. */
async function mount(status: LichessStatus) {
  const calls: { method: string; path: string; body: unknown }[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: Request | string) => {
      const req = typeof input === "string" ? new Request(new URL(input, "http://x")) : input;
      calls.push({ method: req.method, path: new URL(req.url).pathname, body: await req.clone().text() });
      return Response.json(status);
    }),
  );
  const root = document.createElement("div");
  document.body.append(root);
  app = createApp(SeekPanel, { status })
    .use(createEos({ locale: "en-US", applyTheme: false }))
    .use(createPinia())
    .use(PiniaColada);
  app.mount(root);
  await nextTick();
  return { root, calls };
}
const button = (root: HTMLElement, text: string) =>
  [...root.querySelectorAll("button")].find((b) => b.textContent?.trim() === text);

describe("seek panel", () => {
  it("offers Find a game when the bot is online and idle", async () => {
    const { root, calls } = await mount({ ...load("lichess-connected.json"), playing: [] });
    const find = button(root, "Find a game");
    expect(find?.disabled).toBe(false);
    find?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(calls[0]).toMatchObject({ method: "POST", path: "/api/lichess/seek" });
    expect(String(calls[0]?.body)).toContain('"once"');
  });

  it("says why it cannot while the bot is offline", async () => {
    const { root } = await mount(load("lichess-off.json"));
    expect(button(root, "Find a game")?.disabled).toBe(true);
    expect(root.textContent).toContain("Switch the bot online");
  });

  it("shows the challenge waiting and stops it", async () => {
    const { root, calls } = await mount(load("lichess-seeking.json"));
    expect(root.textContent).toContain("Challenged Alpha-Bot (1950) to 3+2 rated.");
    expect(root.textContent).toContain("Alpha-Bot");
    expect(root.textContent).toContain("3+2 · rated");
    expect(button(root, "Find a game")).toBeUndefined();
    button(root, "Stop looking")?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(calls[0]).toMatchObject({ method: "DELETE", path: "/api/lichess/seek" });
  });
});

describe("seek clocks", () => {
  it("reads minutes plus seconds", () => {
    expect(parseClock("3+2")).toEqual({ initial: 180, increment: 2 });
    expect(parseClock("0.5+0")).toEqual({ initial: 30, increment: 0 });
  });

  it("lists the presets shortest first, with clocks set elsewhere", () => {
    const values = clockOptions(["4+1"]).map((o) => o.value);
    expect(values[0]).toBe("1+0");
    expect(values).toContain("4+1");
    expect(values.indexOf("3+2")).toBeLessThan(values.indexOf("10+5"));
  });
});
