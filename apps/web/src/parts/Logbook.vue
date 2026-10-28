<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { RouterLink } from "vue-router";
import { PhCaretDown, PhCaretUp } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";
import { EList } from "@eos-ui/vue/components/EList";
import { EListRow } from "@eos-ui/vue/components/EListRow";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import { ETag } from "@eos-ui/vue/components/ETag";
import { ago } from "../format";
import { ideaElo, KIND_WORD, STATUS_TONE, STATUS_WORD } from "../sheets/idea";
import { useIdeas } from "../stores/ideas";
import Notes from "./Notes.vue";

/**
 * The logbook: every idea that has ended, newest first (the ones still going are above it, under Ideas in progress), with who had it, its kind, how it ended and by how much. Each opens to its
 * hypothesis and notes, and links to its own sheet.
 */
const ideas = useIdeas();

type Show = "ended" | "kept" | "dropped";
const show = ref<Show>("ended");
const open = ref<Set<number>>(new Set());

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

function toggle(id: number) {
  const next = new Set(open.value);
  if (!next.delete(id)) next.add(id);
  open.value = next;
}
</script>

<template>
  <div class="log">
    <ESegmented
      v-model="show"
      label="Show"
      hide-label
      size="sm"
      :options="[
        { value: 'ended', label: 'Ended' },
        { value: 'kept', label: 'Kept' },
        { value: 'dropped', label: 'Dropped' },
      ]"
    />
    <ESkeleton v-if="!ideas.loaded" :lines="3" label="Loading the logbook" />
    <p v-else-if="!rows.length" class="sec-note">{{ empty[show] }}</p>
    <EList v-else :model-value="null" label="Ended ideas">
      <div v-for="i in shown" :key="i.id" class="entry">
        <EListRow
          :value="i.id"
          :title="i.title"
          :subtitle="`${i.author === 'you' ? 'You' : 'Claude'} · ${KIND_WORD[i.kind]} · ${ago(i.created_at)}`"
          :link-component="RouterLink"
          :to="`/dameless/ideas/${i.id}`"
        >
          <template #leading>
            <EStamp size="sm" :tone="STATUS_TONE[i.status]" :label="STATUS_WORD[i.status]" class="stamp" />
          </template>
          <template #trailing>
            <span class="elo mono">{{ ideaElo(i) }}</span>
          </template>
          <template #actions>
            <EIconButton
              :icon="open.has(i.id) ? PhCaretUp : PhCaretDown"
              :label="open.has(i.id) ? `Hide notes on ${i.title}` : `Show notes on ${i.title}`"
              tooltip
              size="xs"
              :aria-expanded="open.has(i.id)"
              @click="toggle(i.id)"
            />
          </template>
        </EListRow>
        <div v-if="open.has(i.id)" class="body">
          <p v-if="i.verdict" class="verdict">{{ i.verdict }}</p>
          <p class="hyp">{{ i.hypothesis }}</p>
          <Notes v-if="i.notes" :text="i.notes" />
          <p v-if="i.tags.length" class="tags">
            <ETag v-for="t in i.tags" :key="t" :label="t" size="sm" />
          </p>
        </div>
      </div>
    </EList>
    <div v-if="rows.length > shown.length" class="more">
      <EButton variant="outline" size="sm" @click="limit += PAGE">
        Show {{ Math.min(PAGE, rows.length - shown.length) }} more
      </EButton>
      <span class="sec-note">{{ shown.length }} of {{ rows.length }}</span>
    </div>
  </div>
</template>

<style scoped>
.log {
  display: grid;
  gap: var(--e-gap-snug);
}

.more {
  display: flex;
  align-items: center;
  gap: var(--e-gap-snug);
}

/* A stamp is as wide as its word; the column keeps every title at one edge. */
.stamp {
  inline-size: 5rem;
}

.elo {
  font-size: 0.8125rem;
}

.body {
  display: grid;
  gap: var(--e-gap-snug);
  padding: var(--e-gap-snug) var(--e-gap-snug) var(--e-gap-snug) calc(5rem + 2 * var(--e-gap-snug));
}

.verdict {
  margin: 0;
  font-weight: 600;
}

.hyp,
.tags {
  margin: 0;
  color: var(--e-muted);
}

.tags {
  display: flex;
  flex-wrap: wrap;
  gap: var(--e-gap-tight);
}

@media (width < 36rem) {
  /* The title takes the room on a phone; the idea's own sheet has the figure. */
  .elo {
    display: none;
  }

  .body {
    padding-inline-start: var(--e-gap-snug);
  }
}
</style>
