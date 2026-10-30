import { ref } from "vue";
import { defineStore } from "pinia";
import { client, ok } from "../api/client";
import type { Health, MachineInfo } from "../api/types";

/** The machine the app runs on, what it has and lacks (Settings › Health), and the app's token. */
export const useMachine = defineStore("machine", () => {
  const machine = ref<MachineInfo | null>(null);
  const token = ref<string | null>(null);
  /** What the machine has and lacks: the analysis engine, Reckless, the disk, the database. */
  const health = ref<Health | null>(null);

  async function loadMachine() {
    machine.value = (await ok(client.GET("/api/status"))).machine;
  }

  async function loadHealth() {
    health.value = await ok(client.GET("/api/diagnostics"));
  }

  async function loadToken() {
    token.value = (await ok(client.GET("/api/token"))).token;
  }

  return { machine, token, health, loadMachine, loadHealth, loadToken };
});
