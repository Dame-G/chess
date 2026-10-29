<script setup lang="ts">
import { computed } from "vue";
import { PhPause, PhPlay, PhStop } from "@phosphor-icons/vue";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";
import { EPopover } from "@eos-ui/vue/components/EPopover";
import { EProgress } from "@eos-ui/vue/components/EProgress";
import type { Work, WorkId } from "../api/types";
import { formatDuration, formatRelative } from "../format";
import { useGames } from "../stores/games";
import { usePolicy } from "../stores/policy";
import { useWork } from "../stores/work";
import UiEmpty from "../ui/UiEmpty.vue";
import UiLive from "../ui/UiLive.vue";
import UiSection from "../ui/UiSection.vue";
import { shortWork } from "./work";

/**
 * Background work in the header: a dot and a short label for what is running ("Trial 412 pairs"), and, when pressed,
 * the whole queue (running, waiting, recent) with pause and stop. It is the only place background work appears, and it
 * carries the fair-play note while analysis is off. Below 75rem the label gives way to the dot.
 */
const work = useWork();
const policy = usePolicy();
const games = useGames();
const toast = useToast();

function progressText(w: Work): string {
  const p = w.progress;
  const parts = [w.title];
  if (p?.text) parts.push(p.text);
  else if (p) parts.push(p.total ? `${p.done} of ${p.total} ${p.unit}` : `${p.done} ${p.unit}`);
  if (w.status === "paused") parts.push(w.paused_for ? `paused for ${w.paused_for.toLowerCase()}` : "paused");
  else if (p?.eta_seconds) parts.push(`~${formatDuration(p.eta_seconds)}`);
  return parts.join(" · ");
}

const first = computed(() => work.running[0]);
/** The header's label: what the first running item is doing, shortly, and how many more there are. */
const label = computed(() => {
  if (games.error) return "Games did not load";
  const w = first.value;
  if (!w) {
    if (work.queued.length) return `${work.queued.length} waiting`;
    return policy.reason ? "Analysis is off" : "";
  }
  const more = work.running.length - 1 + work.queued.length;
  return `${shortWork(w)}${more ? ` +${more}` : ""}`;
});
/** The sentence for the tooltip and the screen reader. */
const sentence = computed(() => {
  if (games.error) return games.error;
  const w = first.value;
  if (!w) return work.queued.length ? `${work.queued.length} waiting` : (policy.reason ?? "Idle");
  const more = work.running.length - 1 + work.queued.length;
  return more ? `${progressText(w)} (and ${more} more)` : progressText(w);
});
const tone = computed<"busy" | "paused" | "error" | "idle">(() => {
  if (games.error) return "error";
  if (work.running.some((w) => w.status === "running")) return "busy";
  if (work.running.length || policy.reason) return "paused";
  return "idle";
});

async function run(f: (id: WorkId) => Promise<unknown>, id: WorkId) {
  try {
    await f(id);
  } catch (e) {
    toast.show({
      title: "That did not go through",
      message: e instanceof Error ? e.message : String(e),
      tone: "warning",
    });
  }
}
const fraction = (w: Work) => (w.progress?.total ? w.progress.done / w.progress.total : undefined);
const empty = computed(() => !work.running.length && !work.queued.length && !work.recent.length);
</script>

<template>
  <EPopover label="Background work" title="Background work" side="bottom" align="end" class="work-panel">
    <template #trigger>
      <EButton
        variant="quiet"
        size="sm"
        class="work"
        :class="`work--${tone}`"
        :aria-label="`Background work: ${sentence}`"
        :title="sentence"
      >
        <span class="dot" aria-hidden="true">
          <UiLive v-if="tone === 'busy'" :pulse="first?.progress?.done" class="dot-live" />
          <span v-else class="dot-still" />
        </span>
        <span v-if="label" class="label" aria-hidden="true">{{ label }}</span>
      </EButton>
    </template>
    <div class="queue">
      <p v-if="policy.reason" class="note" role="status">{{ policy.reason }}</p>
      <UiSection v-if="work.running.length" title="Running" :level="2">
        <ul class="rows">
          <li v-for="w in work.running" :key="w.id" class="row">
            <span class="row-text">{{ progressText(w) }}</span>
            <EProgress
              v-if="fraction(w) !== undefined"
              :value="fraction(w)! * 100"
              :label="w.title"
              hide-label
              class="bar"
            />
            <span class="row-actions">
              <EIconButton
                v-if="w.status === 'running'"
                :icon="PhPause"
                :label="`Pause ${w.title}`"
                tooltip
                size="sm"
                @click="run(work.pause, w.id)"
              />
              <EIconButton
                v-else
                :icon="PhPlay"
                :label="`Resume ${w.title}`"
                tooltip
                size="sm"
                @click="run(work.resume, w.id)"
              />
              <EIconButton :icon="PhStop" :label="`Stop ${w.title}`" tooltip size="sm" @click="run(work.stop, w.id)" />
            </span>
          </li>
        </ul>
      </UiSection>
      <UiSection v-if="work.queued.length" title="Waiting" :level="2">
        <ul class="rows">
          <li v-for="w in work.queued" :key="w.id" class="row">
            <span class="row-text">{{ w.title }}</span>
            <span class="row-actions">
              <EIconButton
                :icon="PhStop"
                :label="`Cancel ${w.title}`"
                tooltip
                size="sm"
                @click="run(work.stop, w.id)"
              />
            </span>
          </li>
        </ul>
      </UiSection>
      <UiSection v-if="work.recent.length" title="Recent" :level="2">
        <ul class="rows">
          <li v-for="w in work.recent" :key="w.id" class="row row--done">
            <span class="row-text">{{ w.title }}</span>
            <span class="row-when" :class="{ failed: w.status === 'failed' }">
              {{ w.status === "failed" ? "Failed" : w.status === "stopped" ? "Stopped" : "Done" }}
              {{ w.ended_at ? formatRelative(w.ended_at) : "" }}
            </span>
          </li>
        </ul>
      </UiSection>
      <UiEmpty v-if="empty" dense title="Idle." body="Nothing has run yet." />
    </div>
  </EPopover>
</template>

<style scoped>
.work {
  max-inline-size: 16rem;
  min-inline-size: 0;
  font: var(--t-body);
  font-weight: 400;
  text-decoration: none;
}

.work :deep(.e-button-label) {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-2);
  overflow: hidden;
  min-inline-size: 0;
}

.dot {
  display: inline-grid;
  place-items: center;
  flex: none;
  inline-size: var(--sp-2);
  block-size: var(--sp-2);
}

.dot-live,
.dot-still {
  display: block;
}

.dot-still {
  inline-size: var(--sp-2);
  block-size: var(--sp-2);
  border-radius: var(--r-pill);
  background: var(--c-ink-3);
}

.work--paused .dot-still {
  background: var(--c-warn);
}

.work--error .dot-still {
  background: var(--c-loss);
}

.label {
  overflow: hidden;
  color: var(--c-ink-2);
  font: var(--t-meta);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.queue {
  display: grid;
  gap: var(--sp-3);
  min-inline-size: min(22rem, 80vw);
}

.note {
  margin: 0;
  color: var(--c-warn);
  font: var(--t-body-strong);
}

.rows {
  display: grid;
  margin: 0;
  padding: 0;
  list-style: none;
}

.row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: center;
  gap: 0 var(--sp-2);
  padding-block: var(--sp-1);
  border-block-end: var(--e-stroke-hair) solid var(--c-line);
}

.row:last-child {
  border-block-end: 0;
}

.row-text {
  min-width: 0;
}

.bar {
  grid-column: 1;
}

.row-actions {
  display: flex;
  grid-row: 1 / span 2;
  grid-column: 2;
}

.row-when {
  color: var(--c-ink-2);
  font: var(--t-meta);
  white-space: nowrap;
}

.row-when.failed {
  color: var(--c-loss);
}

/* Below 75rem the header has no room for words: the dot alone says it, and the sentence is the tooltip. */
@media (width < 75rem) {
  .label {
    display: none;
  }
}
</style>
