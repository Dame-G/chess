import { ref } from "vue";
import { defineStore } from "pinia";
import { client, ok } from "../api/client";
import type {
  DamelessView,
  HarnessStatus,
  Health,
  LevelsOverview,
  MachineInfo,
  PlannedMatch,
  Pruned,
} from "../api/types";
import { listen, reloadOnReconnect } from "./listen";

/**
 * Dameless as a whole: the champion, its line and strength, and the machine it runs on. The overview is read when the
 * Dameless sheet opens and again when a version or an idea changes under it.
 */
export const useDameless = defineStore("dameless", () => {
  const overview = ref<DamelessView | null>(null);
  const machine = ref<MachineInfo | null>(null);
  const token = ref<string | null>(null);
  /** The strength ladder (`dameless@<level>` seats) with each level's rating, and the level suggested for You. */
  const levels = ref<LevelsOverview | null>(null);
  /** The calibration matches that would teach the most now, with how many games each plays. */
  const plan = ref<PlannedMatch[]>([]);
  /** What the machine has and lacks for the work (Settings › Health): fastchess, the book, the engine, the disk… */
  const health = ref<Health | null>(null);
  /** Whether results on this machine can be trusted: the last check of the harness, and one running. */
  const harness = ref<HarnessStatus | null>(null);
  let wanted = false;

  async function load() {
    wanted = true;
    overview.value = await ok(client.GET("/api/dameless"));
  }

  async function loadMachine() {
    machine.value = (await ok(client.GET("/api/status"))).machine;
  }

  async function loadHealth() {
    health.value = await ok(client.GET("/api/diagnostics"));
  }

  async function loadHarness() {
    harness.value = await ok(client.GET("/api/harness"));
  }

  /** Starts a check of the harness (an idea, tag `harness`): `main` against a branch identical to it. */
  async function checkHarness(pairs?: number) {
    const idea = await ok(client.POST("/api/harness/check", { body: { pairs: pairs ?? null } }));
    await loadHarness();
    return idea;
  }

  async function loadLevels() {
    levels.value = await ok(client.GET("/api/levels"));
  }

  async function loadPlan(count = 3) {
    plan.value = await ok(client.GET("/api/levels/plan", { params: { query: { count } } }));
  }

  /** Queues calibration of the levels (background work at the lowest priority); answers the work. */
  const calibrate = (count = 3) => ok(client.POST("/api/levels/calibrate", { body: { count } }));

  async function loadToken() {
    token.value = (await ok(client.GET("/api/token"))).token;
  }

  /** Pushes the fork's `main` to `origin`: explicit, never automatic. */
  const push = () => ok(client.POST("/api/dameless/push"));

  /** Frees the binaries nothing uses; with `dry_run` only says how many and how much. */
  async function prune(dryRun: boolean): Promise<Pruned> {
    const result = await ok(client.POST("/api/builds/prune", { body: { dry_run: dryRun } }));
    if (!dryRun) await loadMachine();
    return result;
  }

  const again = () => {
    if (wanted) void load().catch(() => {});
  };
  listen("levels.updated", () => {
    if (levels.value) void loadLevels().catch(() => {});
  });
  listen("version.upserted", again);
  listen("idea.upserted", (idea) => {
    again();
    if (harness.value && idea.tags.includes("harness")) void loadHarness().catch(() => {});
  });
  reloadOnReconnect(async () => again());

  return {
    overview,
    machine,
    token,
    levels,
    plan,
    health,
    harness,
    load,
    loadMachine,
    loadLevels,
    loadPlan,
    calibrate,
    loadHealth,
    loadHarness,
    checkHarness,
    loadToken,
    push,
    prune,
  };
});
