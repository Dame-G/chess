<script setup lang="ts">
import { onUnmounted } from "vue";
import { RouterLink, RouterView, useRoute } from "vue-router";
import { useQueryCache } from "@pinia/colada";
import { EAppHeader } from "@eos-ui/vue/components/EAppHeader";
import { EAppShell } from "@eos-ui/vue/components/EAppShell";
import { ENavList } from "@eos-ui/vue/components/ENavList";
import { followEvents } from "./api/events";

const route = useRoute();
const items = [{ label: "Runner", to: "/" }];

// One live stream for the whole app. A restarted runner may have a new build or schema, so everything refetches.
const cache = useQueryCache();
const stop = followEvents((event) => {
  if (event.kind === "runner.started") cache.invalidateQueries();
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
          :is-current="(item) => item.to === route.path"
        />
      </EAppHeader>
    </template>
    <RouterView />
  </EAppShell>
</template>
