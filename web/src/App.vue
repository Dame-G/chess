<script setup lang="ts">
import { onUnmounted } from "vue";
import { RouterLink, RouterView, useRoute } from "vue-router";
import { useQueryCache } from "@pinia/colada";
import { EAppHeader } from "@eos-ui/vue/components/EAppHeader";
import { EAppShell } from "@eos-ui/vue/components/EAppShell";
import { ENavList } from "@eos-ui/vue/components/ENavList";
import { dispatch, followEvents } from "./api/events";

const route = useRoute();
const items = [
  { label: "Experiments", to: "/experiments" },
  { label: "Analysis", to: "/analysis" },
  { label: "Engines", to: "/engines" },
  { label: "Runner", to: "/" },
];

// One live stream for the whole app. A restarted runner may have a new build or schema, so everything refetches.
const cache = useQueryCache();
const stop = followEvents((event) => {
  if (event.kind === "runner.started") cache.invalidateQueries();
  if (event.kind === "job.updated") {
    cache.invalidateQueries({ key: ["jobs"] });
    if ((event.data as { status?: string }).status === "done") cache.invalidateQueries({ key: ["builds"] });
  }
  if (event.kind === "experiment.updated") {
    const { id } = event.data as { id: number };
    cache.invalidateQueries({ key: ["experiments"] });
    cache.invalidateQueries({ key: ["experiment", id] });
  }
  if (event.kind === "test.progress") {
    const { id } = event.data as { id: number };
    cache.invalidateQueries({ key: ["test", id] });
    cache.invalidateQueries({ key: ["games", id] });
  }
  dispatch(event);
});
onUnmounted(stop);
</script>

<template>
  <EAppShell>
    <template #header>
      <EAppHeader sticky brand="chess" brand-to="/" :link-component="RouterLink">
        <ENavList
          label="Main"
          orientation="horizontal"
          :link-component="RouterLink"
          :items="items"
          :is-current="(item) => (item.to === '/' ? route.path === '/' : route.path.startsWith(String(item.to)))"
        />
      </EAppHeader>
    </template>
    <RouterView />
  </EAppShell>
</template>
