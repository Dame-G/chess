<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { RouterLink } from "vue-router";
import { EButton } from "@eos-ui/vue/components/EButton";
import type { Idea } from "../api/types";
import { formatRelative } from "../format";
import { ideaElo, KIND_WORD, STATUS_WORD } from "../sheets/idea";
import { useIdeas } from "../stores/ideas";
import { ideaTone } from "../ui/badge";
import UiBadge from "../ui/UiBadge.vue";
import UiSection from "../ui/UiSection.vue";
import UiSeg from "../ui/UiSeg.vue";
import UiTable from "../ui/UiTable.vue";
import Notes from "./Notes.vue";

/**
 * The logbook: every idea that has ended, newest first (the ones still going are above it), with who had it, its kind,
 * how it ended and by how much. A row opens to its verdict, hypothesis and notes, with a link to the idea itself.
 */
const ideas = useIdeas();

type Show = "ended" | "kept" | "dropped";
const show = ref<Show>("ended");
const open = ref<string[]>([]);

const rows = computed(() =>
  ideas.list.filter((i) => {
    switch (show.value) {
      case "kept":
        return i.status === "kept";
      case "dropped":
        return i.status === "dropped";
      default:
        return i.status !== "running" && i.status !== "queued";
    }
  }),
);

const empty: Record<Show, string> = {
  ended: "No idea has ended yet. Each one is written down here with how it ended.",
  kept: "No idea has been kept yet.",
  dropped: "No idea has been dropped yet.",
};

/** A long logbook shows its newest page first, so the sheet stays short and tabbing through it stays quick. */
const PAGE = 25;
const limit = ref(PAGE);
watch(show, () => (limit.value = PAGE));
const shown = computed(() => rows.value.slice(0, limit.value));

const columns = [
  { key: "title", label: "Idea" },
  { key: "kind", label: "Kind", hide: "sm" as const },
  { key: "status", label: "Result" },
  { key: "elo", label: "Elo", fig: true, hide: "sm" as const },
  { key: "when", label: "When", fig: true, hide: "md" as const },
];

function toggle(idea: Idea) {
  const key = String(idea.id);
  open.value = open.value.includes(key) ? open.value.filter((k) => k !== key) : [...open.value, key];
}
const who = (i: Idea) => (i.author === "you" ? "You" : "Claude");
</script>

<template>
  <UiSection title="Logbook">
    <template #aside>
      <UiSeg
        v-model="show"
        label="Show ended ideas"
        size="sm"
        :options="[
          { value: 'ended', label: 'Ended' },
          { value: 'kept', label: 'Kept' },
          { value: 'dropped', label: 'Dropped' },
        ]"
      />
    </template>
    <UiTable
      label="Ended ideas"
      :columns="columns"
      :rows="shown"
      row-key="id"
      :expanded="open"
      :loading="!ideas.loaded"
      :empty="empty[show]"
      @select="toggle"
    >
      <template #cell-title="{ row }">
        <span class="title">{{ row.title }}</span>
        <span class="by">{{ who(row) }}</span>
      </template>
      <template #cell-kind="{ row }">{{ KIND_WORD[row.kind] }}</template>
      <template #cell-status="{ row }">
        <UiBadge :tone="ideaTone[row.status]">{{ STATUS_WORD[row.status] }}</UiBadge>
      </template>
      <template #cell-elo="{ row }">{{ ideaElo(row) || "–" }}</template>
      <template #cell-when="{ row }">{{ formatRelative(row.created_at) }}</template>
      <template #row-aside="{ row }">
        <div class="body">
          <p v-if="row.verdict" class="verdict">{{ row.verdict }}</p>
          <p class="hyp">{{ row.hypothesis }}</p>
          <Notes v-if="row.notes" :text="row.notes" />
          <p v-if="row.tags.length" class="tags">
            <UiBadge v-for="t in row.tags" :key="t">{{ t }}</UiBadge>
          </p>
          <RouterLink :to="`/dameless/ideas/${row.id}`" class="open">Open the idea</RouterLink>
        </div>
      </template>
    </UiTable>
    <div v-if="rows.length > shown.length" class="more">
      <EButton variant="outline" size="sm" @click="limit += PAGE">
        Show {{ Math.min(PAGE, rows.length - shown.length) }} more
      </EButton>
      <span class="count">{{ shown.length }} of {{ rows.length }}</span>
    </div>
  </UiSection>
</template>

<style scoped>
.title {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
}

.by {
  display: block;
  color: var(--c-ink-2);
  font: var(--t-meta);
  font-weight: 400;
}

.more {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  padding-block-start: var(--sp-2);
}

.count {
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.body {
  display: grid;
  gap: var(--sp-2);
  padding: var(--sp-2) var(--sp-3) var(--sp-3);
}

.verdict {
  margin: 0;
  font: var(--t-body-strong);
}

.hyp,
.tags {
  margin: 0;
  color: var(--c-ink-2);
}

.tags {
  display: flex;
  flex-wrap: wrap;
  gap: var(--sp-1);
}

.open {
  justify-self: start;
}
</style>
