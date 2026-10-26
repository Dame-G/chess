import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createApp, h, nextTick, ref, type App as VueApp, type Component } from "vue";
import { createPinia } from "pinia";
import { PiniaColada } from "@pinia/colada";
import { createEos } from "@eos-ui/vue/plugin";
import { createMemoryHistory, createRouter } from "vue-router";
import "@eos-ui/vue/icons/builtin";
import type { Tune } from "../src/api/queries";

const fixture = (name: string) => readFileSync(join(process.cwd(), "tests", "fixtures", name), "utf8");
const calls: { method: string; path: string; body?: unknown }[] = [];

function runner(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  const request = input instanceof Request ? input : undefined;
  const url = new URL(request?.url ?? String(input), "http://x");
  const method = request?.method ?? init?.method ?? "GET";
  const reply = (body: string) =>
    Promise.resolve(new Response(body, { headers: { "content-type": "application/json" } }));
  if (method !== "GET") {
    return (request ? request.text() : Promise.resolve(String(init?.body ?? ""))).then((text) => {
      calls.push({ method, path: url.pathname, body: text ? JSON.parse(text) : undefined });
      return new Response(JSON.stringify({ id: 3 }), { status: 202, headers: { "content-type": "application/json" } });
    });
  }
  if (url.pathname === "/api/branches") return reply(fixture("branches.json"));
  if (url.pathname === "/api/builds") return reply(fixture("builds.json"));
  if (url.pathname === "/api/status") return reply(fixture("status.json"));
  return Promise.resolve(new Response("{}", { status: 404 }));
}

let mounted: VueApp | undefined;
afterEach(() => {
  mounted?.unmount();
  mounted = undefined;
  document.body.innerHTML = "";
  calls.length = 0;
});

async function mount(component: Component) {
  vi.stubGlobal("fetch", vi.fn(runner));
  const router = createRouter({ history: createMemoryHistory(), routes: [{ path: "/:rest(.*)*", component: {} }] });
  const root = document.createElement("div");
  document.body.append(root);
  const app = createApp(component);
  app
    .use(createEos({ locale: "en-US", applyTheme: false }))
    .use(createPinia())
    .use(PiniaColada)
    .use(router);
  mounted = app;
  app.mount(root);
  for (let i = 0; i < 10; i++) {
    await new Promise((r) => setTimeout(r, 10));
    await nextTick();
  }
}

const click = async (label: RegExp) => {
  const button = [...document.querySelectorAll("button")].find((b) => label.test(b.textContent ?? ""));
  expect(button, String(label)).toBeTruthy();
  button!.click();
  for (let i = 0; i < 10; i++) {
    await new Promise((r) => setTimeout(r, 10));
    await nextTick();
  }
};

describe("tune dialogs", { timeout: 30_000 }, () => {
  it("starts a tune with the defaults, naming only the branch", async () => {
    const { default: NewWork } = await import("../src/components/improve/NewWork.vue");
    await mount({ render: () => h(NewWork, { open: true, mode: "tune" }) });
    expect(document.body.textContent).toContain("How long are the games?");
    expect(document.body.textContent).toContain("16,000 games");
    await click(/Start tune/);
    expect(calls).toHaveLength(1);
    expect(calls[0]!.path).toBe("/api/tunes");
    expect(calls[0]!.body).toEqual({ build: "spsa/tunables", iterations: 1000, pairs_per_iteration: 8, preset: "stc" });
  });

  it("applies a tune to a branch named after it", async () => {
    const { default: ApplyTune } = await import("../src/components/tune/ApplyTune.vue");
    const tunes = JSON.parse(fixture("tunes.json")) as Tune[];
    const open = ref(true);
    await mount({ render: () => h(ApplyTune, { open: open.value, tune: tunes[0]! }) });
    expect(document.body.textContent).toContain("Apply this tune");
    await click(/^\s*Apply\s*$/);
    expect(calls[0]!.path).toBe("/api/tunes/1/apply");
    expect(calls[0]!.body).toEqual({ branch: "tune/1", base: "main", experiment: true, ltc: false, push: false });
  });
});

describe("improve dialogs", { timeout: 30_000 }, () => {
  it("starts a test of the newest fork branch against main", async () => {
    const { default: NewWork } = await import("../src/components/improve/NewWork.vue");
    await mount({ render: () => h(NewWork, { open: true }) });
    expect(document.body.textContent).toContain("Tune numbers");
    expect(document.body.textContent).toContain("Branch to test");
    await click(/Start test/);
    expect(calls).toHaveLength(1);
    expect(calls[0]!.path).toBe("/api/experiments");
    expect(calls[0]!.body).toMatchObject({ dev: "test/comment", base: "main", kind: "gainer" });
  });

  it("lists branches with their commit titles and age, upstream apart", async () => {
    const { default: BranchPicker } = await import("../src/components/BranchPicker.vue");
    const picked = ref("test/no-nmp");
    await mount({
      render: () => h(BranchPicker, { label: "Branch", modelValue: picked.value, exclude: ["main"] }),
    });
    const input = document.querySelector<HTMLInputElement>("input")!;
    expect(input.value).toBe("test/no-nmp");
    input.focus();
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    await new Promise((r) => setTimeout(r, 30));
    const text = document.body.textContent ?? "";
    expect(text).toContain("Clarify the NMP comment");
    expect(text).toContain("Upstream");
    expect(text).not.toContain("Minor code cleanups (#1123)main");
  });
});
