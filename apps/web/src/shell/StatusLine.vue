<script setup lang="ts">
import { computed } from "vue";
import { PhPause, PhPlay, PhStop } from "@phosphor-icons/vue";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";
import { EProgress } from "@eos-ui/vue/components/EProgress";
import { EPopover } from "@eos-ui/vue/components/EPopover";
import type { Work, WorkId } from "../api/types";
import { ago, duration } from "../format";
import { useGames } from "../stores/games";
import { usePolicy } from "../stores/policy";
import { useWork } from "../stores/work";

/**
 * The status line: what background work is doing, in one sentence, and the queue on click (running, queued, recent,
 * with stop and pause). It is the only place background work appears. At its end, the fair-play note when analysis is
 * off.
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
  else if (p?.eta_seconds) parts.push(`~${duration(p.eta_seconds)}`);
  return parts.join(" · ");
}

const sentence = computed(() => {
  if (games.error) return games.error;
  const first = work.running[0];
  if (!first) return work.queued.length ? `${work.queued.length} waiting` : "Nothing running";
  const more = work.running.length - 1 + work.queued.length;
  return more ? `${progressText(first)} (and ${more} more)` : progressText(first);
});
const state = computed(() =>
  work.running.some((w) => w.status === "running") ? "busy" : work.running.length ? "paused" : "idle",
);

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
</script>

<template>
  <div class="status">
    <EPopover label="Background work" title="Background work" side="top" align="start">
      <template #trigger>
        <EButton variant="quiet" size="sm" class="work-trigger">
          <span class="dot" :class="`dot--${state}`" aria-hidden="true"></span>
          <span class="sentence">{{ sentence }}</span>
        </EButton>
      </template>
      <div class="queue">
        <section v-if="work.running.length" aria-label="Running">
          <p class="part-title">Running</p>
          <ul class="rows">
            <li v-for="w in work.running" :key="w.id" class="row">
              <span class="row-text">{{ progressText(w) }}</span>
              <EProgress v-if="fraction(w) !== undefined" :value="fraction(w)! * 100" :label="w.title" class="bar" />
              <span class="row-actions">
                <EIconButton
                  v-if="w.status === 'running'"
                  :icon="PhPause"
                  :label="`Pause ${w.title}`"
                  size="sm"
                  @click="run(work.pause, w.id)"
                />
                <EIconButton
                  v-else
                  :icon="PhPlay"
                  :label="`Resume ${w.title}`"
                  size="sm"
                  @click="run(work.resume, w.id)"
                />
                <EIconButton :icon="PhStop" :label="`Stop ${w.title}`" size="sm" @click="run(work.stop, w.id)" />
              </span>
            </li>
          </ul>
        </section>
        <section v-if="work.queued.length" aria-label="Queued">
          <p class="part-title">Waiting</p>
          <ul class="rows">
            <li v-for="w in work.queued" :key="w.id" class="row">
              <span class="row-text">{{ w.title }}</span>
              <span class="row-actions">
                <EIconButton :icon="PhStop" :label="`Cancel ${w.title}`" size="sm" @click="run(work.stop, w.id)" />
              </span>
            </li>
          </ul>
        </section>
        <section v-if="work.recent.length" aria-label="Recent">
          <p class="part-title">Recent</p>
          <ul class="rows">
            <li v-for="w in work.recent" :key="w.id" class="row row--done">
              <span class="row-text">{{ w.title }}</span>
              <span class="row-when" :class="{ failed: w.status === 'failed' }">
                {{ w.status === "failed" ? "failed" : w.status === "stopped" ? "stopped" : "done" }}
                {{ w.ended_at ? ago(w.ended_at) : "" }}
              </span>
            </li>
          </ul>
        </section>
        <p v-if="!work.running.length && !work.queued.length && !work.recent.length" class="none">
          Nothing has run yet.
        </p>
      </div>
    </EPopover>
    <span v-if="policy.reason" class="fairplay" role="status">{{ policy.reason }}</span>
  </div>
</template>

<style scoped>
.status {
  display: flex;
  overflow: hidden;
  align-items: center;
  gap: var(--e-gap-base);
  min-width: 0;
  min-height: 2rem;
  font: var(--e-type-small);
}

.work-trigger {
  justify-content: flex-start;
  inline-size: auto;
  max-inline-size: 100%;
  min-inline-size: 0;
  font: var(--e-type-small);
  font-weight: 400;
  text-decoration: none;
}

.work-trigger :deep(.e-button-label) {
  display: inline-flex;
  align-items: center;
  overflow: hidden;
  min-inline-size: 0;
}

.sentence {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dot {
  display: inline-block;
  flex: none;
  width: 0.5rem;
  height: 0.5rem;
  margin-inline-end: 0.5rem;
  border-radius: 50%;
  background: var(--e-muted);
}

.dot--busy {
  background: var(--e-success);
}

.dot--paused {
  background: var(--e-warning);
}

.fairplay {
  margin-inline-start: auto;
  color: var(--e-warning-ink, var(--e-warning));
  font-weight: 600;
  white-space: nowrap;
}

.queue {
  display: grid;
  gap: var(--e-gap-base);
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
  gap: 0 var(--e-gap-snug);
  padding-block: var(--e-gap-tight);
  border-block-end: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 50%, transparent);
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
  color: var(--e-muted);
  white-space: nowrap;
}

.row-when.failed {
  color: var(--e-danger);
}

.none {
  margin: 0;
  color: var(--e-muted);
}

@media (width < 40rem) {
  .fairplay {
    white-space: normal;
  }
}
</style>
