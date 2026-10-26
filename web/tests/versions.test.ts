import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createApp, h, nextTick, ref, type App as VueApp, type Component } from "vue";
import { createPinia } from "pinia";
import { PiniaColada } from "@pinia/colada";
import { createEos } from "@eos-ui/vue/plugin";
import { createMemoryHistory, createRouter } from "vue-router";
import "@eos-ui/vue/icons/builtin";
import { defaultBuild, versionPath, type Build, type Version } from "../src/api/queries";
import { libraryOf, resolveVersion, verdictStatus } from "../src/components/versions/versions";

const fixture = (name: string) => readFileSync(join(process.cwd(), "tests", "fixtures", name), "utf8");
const versions = JSON.parse(fixture("versions.json")) as Version[];
const by = (name: string) => versions.find((v) => v.name === name)!;

describe("the versions library", () => {
  it("hides hidden versions unless asked, and keeps the open one", () => {
    const names = (list: Version[]) => list.map((v) => v.name);
    expect(names(libraryOf(versions, "", false))).not.toContain("upstream/main");
    expect(names(libraryOf(versions, "", true))).toContain("upstream/main");
    expect(names(libraryOf(versions, "", false, "upstream/main"))).toContain("upstream/main");
  });

  it("puts main first, then the fork's branches, then upstream's", () => {
    const list = libraryOf(versions, "", true);
    expect(list[0]!.name).toBe("main");
    expect(list.findIndex((v) => v.upstream)).toBeGreaterThan(list.findLastIndex((v) => !v.upstream));
  });

  it("searches names and commit titles", () => {
    expect(libraryOf(versions, "null move", true).map((v) => v.name)).toEqual(["test/no-nmp"]);
    expect(libraryOf(versions, "NO-NMP", true).map((v) => v.name)).toEqual(["test/no-nmp"]);
  });

  it("reads a version's verdict in a word", () => {
    expect(verdictStatus(by("test/no-nmp")).label).toBe("Rejected");
    expect(verdictStatus(by("test/comment")).label).toBe("Passed");
    expect(verdictStatus(by("spsa/tunables")).label).toBe("Untested");
  });

  it("sends an old build address to the branch that owns the build", () => {
    expect(resolveVersion(versions, "4")?.name).toBe("test/no-nmp");
    expect(resolveVersion(versions, "2")?.name).toBe("main");
    expect(resolveVersion(versions, "test/no-nmp")?.name).toBe("test/no-nmp");
    expect(resolveVersion(versions, "nope")).toBeUndefined();
  });

  it("keeps a slash in a name as one path segment", () => {
    expect(versionPath("test/no-nmp")).toBe("/versions/test%2Fno-nmp");
  });

  it("never picks a deleted build as the default engine", () => {
    const build = (id: number, ref: string, deleted: boolean) =>
      ({ id, ref, deleted_at: deleted ? "x" : null }) as Build;
    expect(defaultBuild([build(3, "main", true), build(2, "other", false)])?.id).toBe(2);
    expect(defaultBuild([build(3, "main", true)])).toBeUndefined();
  });
});

const calls: { method: string; path: string; search: string; body?: unknown }[] = [];
function runner(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  const request = input instanceof Request ? input : undefined;
  const url = new URL(request?.url ?? String(input), "http://x");
  const method = request?.method ?? init?.method ?? "GET";
  const reply = (body: string) =>
    Promise.resolve(new Response(body, { headers: { "content-type": "application/json" } }));
  if (method !== "GET") {
    return (request ? request.text() : Promise.resolve(String(init?.body ?? ""))).then((text) => {
      const body = text ? (JSON.parse(text) as { dry_run?: boolean }) : undefined;
      calls.push({ method, path: url.pathname, search: url.search, body });
      if (url.pathname === "/api/builds/prune")
        return Response.json({ deleted: [1, 2], in_use: [3], freed_bytes: 412_000_000, dry_run: !!body?.dry_run });
      if (url.pathname.startsWith("/api/builds/") && method === "DELETE")
        return Response.json({ error: "build 2 is in use by a running test" }, { status: 409 });
      return Response.json({ name: "test/no-nmp", builds: [], remote: false, id: 9 });
    });
  }
  if (url.pathname === "/api/versions") return reply(fixture("versions.json"));
  if (url.pathname === "/api/branches") return reply(fixture("branches.json"));
  if (url.pathname === "/api/builds") return reply(fixture("builds.json"));
  if (url.pathname === "/api/jobs") return reply("[]");
  if (url.pathname === "/api/matches") return reply("[]");
  return Promise.resolve(new Response("{}", { status: 404 }));
}

let mounted: VueApp | undefined;
afterEach(() => {
  mounted?.unmount();
  mounted = undefined;
  document.body.innerHTML = "";
  calls.length = 0;
});

async function settle() {
  for (let i = 0; i < 10; i++) {
    await new Promise((r) => setTimeout(r, 10));
    await nextTick();
  }
}
async function mount(component: Component, path = "/") {
  vi.stubGlobal("fetch", vi.fn(runner));
  const router = createRouter({ history: createMemoryHistory(), routes: [{ path: "/:rest(.*)*", component: {} }] });
  await router.push(path);
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
  await settle();
}
const button = (label: RegExp) => [...document.querySelectorAll("button")].find((b) => label.test(b.textContent ?? ""));
const click = async (label: RegExp) => {
  const b = button(label);
  expect(b, String(label)).toBeTruthy();
  b!.click();
  await settle();
};

describe("version dialogs", { timeout: 30_000 }, () => {
  it("only deletes a branch once its name is typed", async () => {
    const { default: DeleteVersion } = await import("../src/components/versions/DeleteVersion.vue");
    await mount({ render: () => h(DeleteVersion, { open: true, version: by("test/no-nmp") }) });
    expect(document.body.textContent).toContain("Also delete it on GitHub");
    expect(button(/^\s*Delete branch\s*$/)!.disabled).toBe(true);
    const input = document.querySelector<HTMLInputElement>("input[type=text], input:not([type])")!;
    input.value = "test/no-nm";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    await settle();
    expect(button(/^\s*Delete branch\s*$/)!.disabled).toBe(true);
    input.value = "test/no-nmp";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    await settle();
    expect(button(/^\s*Delete branch\s*$/)!.disabled).toBe(false);
    await click(/^\s*Delete branch\s*$/);
    expect(calls[0]).toMatchObject({
      method: "DELETE",
      path: "/api/versions/test%2Fno-nmp",
      body: { confirm: "test/no-nmp" },
    });
  });

  it("asks what a prune would delete before deleting anything", async () => {
    const { default: FreeSpace } = await import("../src/components/versions/FreeSpace.vue");
    const open = ref(true);
    await mount({ render: () => h(FreeSpace, { open: open.value }) });
    expect(calls).toHaveLength(1);
    expect(calls[0]!.body).toMatchObject({ dry_run: true, keep_latest_per_branch: true });
    expect(document.body.textContent).toContain("Would free 412 MB");
    expect(document.body.textContent).toContain("1 build would match but is in use");
    await click(/Delete 2 builds/);
    expect(calls[1]!.body).toMatchObject({ dry_run: false });
    expect(document.body.textContent).toContain("Freed 412 MB");
  });

  it("shows the reason when a build is in use", async () => {
    const { default: BuildTable } = await import("../src/components/versions/BuildTable.vue");
    await mount({ render: () => h(BuildTable, { version: by("test/no-nmp") }) });
    await click(/^\s*Delete\s*$/);
    await click(/Delete build/);
    expect(calls[0]).toMatchObject({ method: "DELETE" });
    expect(document.body.textContent).toContain("in use");
  });
});

describe("the board's version pickers", { timeout: 30_000 }, () => {
  it("plays the newest build of the chosen version", async () => {
    const { default: GameSetup } = await import("../src/board/GameSetup.vue");
    await mount(
      { render: () => h(GameSetup, { mode: "play", fen: "startpos", moves: [] }) },
      "/board?opponent=test%2Fno-nmp",
    );
    expect(document.body.textContent).toContain("Play against");
    expect((document.querySelector("input") as HTMLInputElement).value).toBe("test/no-nmp");
    await click(/^\s*Play\s*$/);
    const post = calls.find((c) => c.path === "/api/matches")!;
    expect(JSON.stringify(post.body)).toMatch(/"(white|black)":4/);
  });

  it("says when a version has no build, and offers to build it", async () => {
    const { default: GameSetup } = await import("../src/board/GameSetup.vue");
    await mount(
      { render: () => h(GameSetup, { mode: "play", fen: "startpos", moves: [] }) },
      "/board?opponent=spsa%2Ftunables",
    );
    expect(document.body.textContent).toContain("spsa/tunables has no build yet");
    expect(button(/^\s*Play\s*$/)!.disabled).toBe(true);
    await click(/Build it/);
    expect(calls[0]).toMatchObject({ path: "/api/builds", body: { ref: "spsa/tunables" } });
  });
});
