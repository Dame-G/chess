<script setup lang="ts">
import { computed } from "vue";
import { RouterLink } from "vue-router";
import { EDescriptionList } from "@eos-ui/vue/components/EDescriptionList";
import { ELink } from "@eos-ui/vue/components/ELink";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import type { IdeaStatus } from "../api/types";
import { useConnections } from "../stores/connections";
import { useIdeas } from "../stores/ideas";
import { useVersions } from "../stores/versions";
import SheetFrame from "./SheetFrame.vue";

/**
 * Dameless: the strength line, the champion, ideas running and queued, the logbook, New idea, and Out in the world
 * (the bot). For now: the champion, the bot's state and the logbook as a list; the strength chart and the forms come
 * next.
 */
const versions = useVersions();
const ideas = useIdeas();
const connections = useConnections();

const facts = computed(() => [
  {
    label: "Champion",
    text: versions.current ? `${versions.current.name} · ${versions.line.length} versions on the line` : "None yet",
  },
  {
    label: "On Lichess",
    text: connections.bot.account
      ? `${connections.bot.account.username} · ${connections.bot.state}${connections.botRating ? ` · ${connections.botRating}` : ""}`
      : "No bot account linked",
  },
  {
    label: "Ideas",
    text: `${ideas.running.length} running, ${ideas.queued.length} waiting, ${ideas.list.length} in all`,
  },
]);
const tone: Record<IdeaStatus, "neutral" | "info" | "success" | "warning" | "danger"> = {
  queued: "neutral",
  running: "info",
  kept: "success",
  dropped: "neutral",
  stopped: "neutral",
  failed: "danger",
};
</script>

<template>
  <SheetFrame
    title="Dameless"
    description="The engine: its strength over its versions, the ideas being tried, and its games on Lichess."
  >
    <EDescriptionList :items="facts" />
    <h2 class="part-title log-title">Logbook</h2>
    <ul class="log">
      <li v-for="i in ideas.list" :key="i.id" class="entry">
        <ELink :link-component="RouterLink" :to="`/dameless/ideas/${i.id}`">{{ i.title }}</ELink>
        <EStamp size="sm" :tone="tone[i.status]" :label="i.status" />
        <p class="verdict">{{ i.verdict ?? i.hypothesis }}</p>
      </li>
    </ul>
  </SheetFrame>
</template>

<style scoped>
.log-title {
  margin-block: var(--e-gap-loose) var(--e-gap-snug);
}

.log {
  display: grid;
  margin: 0;
  padding: 0;
  list-style: none;
}

.entry {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: var(--e-gap-tight) var(--e-gap-snug);
  padding-block: var(--e-gap-snug);
  border-block-end: var(--line);
}

.verdict {
  grid-column: 1 / -1;
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
