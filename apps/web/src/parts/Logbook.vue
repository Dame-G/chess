<script setup lang="ts">
import { computed, ref } from "vue";
import { RouterLink } from "vue-router";
import { PhCaretDown, PhCaretUp } from "@phosphor-icons/vue";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import { ETag } from "@eos-ui/vue/components/ETag";
import { ago } from "../format";
import { ideaElo, KIND_WORD, STATUS_TONE, STATUS_WORD } from "../sheets/idea";
import { useIdeas } from "../stores/ideas";
import Notes from "./Notes.vue";

/**
 * The logbook: every idea, newest first, with who had it, its kind, how it ended and by how much. Each opens to its
 * hypothesis and notes, and links to its own sheet.
 */
const ideas = useIdeas();

type Show = "all" | "kept" | "dropped" | "active";
const show = ref<Show>("all");
const open = ref<Set<number>>(new Set());

const rows = computed(() =>
  ideas.list.filter((i) => {
    switch (show.value) {
      case "kept":
        return i.status === "kept";
      case "dropped":
        return i.status === "dropped";
      case "active":
        return i.status === "running" || i.status === "queued";
      default:
        return true;
    }
  }),
);

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
        { value: 'all', label: 'All' },
        { value: 'kept', label: 'Kept' },
        { value: 'dropped', label: 'Dropped' },
        { value: 'active', label: 'Active' },
      ]"
    />
    <EEmptyState v-if="!rows.length" title="Nothing here yet">
      Ideas you or Claude try are written down here, with how they ended.
    </EEmptyState>
    <ul v-else class="plain">
      <li v-for="i in rows" :key="i.id" class="entry">
        <div class="head">
          <EStamp size="sm" :tone="STATUS_TONE[i.status]" :label="STATUS_WORD[i.status]" class="stamp" />
          <div class="titles">
            <ELink :link-component="RouterLink" :to="`/dameless/ideas/${i.id}`" class="title">{{ i.title }}</ELink>
            <span class="sub">
              {{ i.author === "you" ? "You" : "Claude" }} · {{ KIND_WORD[i.kind] }} · {{ ago(i.created_at) }}
            </span>
          </div>
          <span class="elo mono">{{ ideaElo(i) }}</span>
          <EIconButton
            :icon="open.has(i.id) ? PhCaretUp : PhCaretDown"
            :label="open.has(i.id) ? `Hide notes on ${i.title}` : `Show notes on ${i.title}`"
            tooltip
            size="sm"
            :aria-expanded="open.has(i.id)"
            @click="toggle(i.id)"
          />
        </div>
        <div v-if="open.has(i.id)" class="body">
          <p v-if="i.verdict" class="verdict">{{ i.verdict }}</p>
          <p class="hyp">{{ i.hypothesis }}</p>
          <Notes v-if="i.notes" :text="i.notes" />
          <p v-if="i.tags.length" class="tags">
            <ETag v-for="t in i.tags" :key="t" :label="t" size="sm" />
          </p>
        </div>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.log {
  display: grid;
  gap: var(--e-gap-snug);
}

.entry {
  padding-block: var(--e-gap-snug);
  border-block-end: var(--line);
}

.head {
  display: grid;
  grid-template-columns: 5.5rem minmax(0, 1fr) auto auto;
  align-items: center;
  gap: var(--e-gap-snug);
}

.stamp {
  justify-self: start;
}

.titles {
  display: grid;
  min-width: 0;
}

.title {
  overflow: hidden;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sub {
  color: var(--e-muted);
  font: var(--e-type-small);
}

.elo {
  color: var(--e-muted);
  font-size: 0.8125rem;
  text-align: end;
}

.body {
  display: grid;
  gap: var(--e-gap-snug);
  margin-block-start: var(--e-gap-snug);
  padding-inline-start: calc(5.5rem + var(--e-gap-snug));
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
  .head {
    grid-template-columns: minmax(0, 1fr) auto;
  }

  .stamp {
    grid-column: 1 / -1;
  }

  .elo {
    display: none;
  }

  .body {
    padding-inline-start: 0;
  }
}
</style>
