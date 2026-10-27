import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { client, ok } from "../api/client";
import type { Version, VersionId } from "../api/types";
import { listen, reloadOnReconnect } from "./listen";

/** The versions of Dameless that matter: the champion line, ideas' changes, upstream tips. */
export const useVersions = defineStore("versions", () => {
  const versions = ref<Record<VersionId, Version>>({});
  const loaded = ref(false);

  /** Newest first. */
  const list = computed(() => Object.values(versions.value).sort((a, b) => b.created_at.localeCompare(a.created_at)));
  /** The champion line, v1 first. */
  const line = computed(() =>
    Object.values(versions.value)
      .filter((v) => v.line.type === "champion")
      .sort((a, b) => champion(a) - champion(b)),
  );
  /** The current best version: the highest number on the champion line. */
  const current = computed(() => line.value.at(-1) ?? null);

  const name = (id: VersionId | null | undefined) => (id == null ? undefined : versions.value[id]?.name);

  async function load() {
    const all = await ok(client.GET("/api/versions"));
    const next: Record<VersionId, Version> = {};
    for (const v of all) next[v.id] = v;
    versions.value = next;
    loaded.value = true;
  }

  listen("version.upserted", (v) => {
    versions.value[v.id] = v;
  });
  reloadOnReconnect(load);

  return { versions, list, line, current, loaded, name, load };
});

function champion(v: Version): number {
  return v.line.type === "champion" ? v.line.number : 0;
}
