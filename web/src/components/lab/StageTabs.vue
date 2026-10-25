<script setup lang="ts">
import { nextTick, onMounted, ref, useId, watch } from "vue";
import { RouterLink, useRoute, useRouter } from "vue-router";
import { PhCheck, PhMinus, PhX } from "@phosphor-icons/vue";
import type { FunnelStage } from "./lab";

/**
 * The funnel as the header of the evidence below it. Each stage is a tab set like a metric: its name as a kicker with a
 * status mark, its headline figure in big mono figures. The chosen stage lives in the URL (`?stage=`), so each tab is
 * a link; stages not reached yet cannot be chosen.
 */
const props = defineProps<{ stages: FunnelStage[]; stage: string; panel: string }>();

const route = useRoute();
const router = useRouter();
const uid = useId();
const list = ref<HTMLElement>();

const tabId = (name: string) => `${uid}-${name}`;
const to = (name: string) => ({ query: { ...route.query, stage: name } });

const words = {
  done: "passed",
  error: "failed",
  current: "running",
  stopped: "stopped",
  upcoming: "not reached",
} as const;

/** Arrow keys move through the stages that can be chosen, as a tab list does. */
async function keys(event: KeyboardEvent) {
  const open = props.stages.filter((s) => s.state !== "upcoming");
  const at = open.findIndex((s) => s.name === props.stage);
  const next =
    event.key === "ArrowRight"
      ? at + 1
      : event.key === "ArrowLeft"
        ? at - 1
        : event.key === "Home"
          ? 0
          : event.key === "End"
            ? open.length - 1
            : null;
  if (next === null || !open.length) return;
  event.preventDefault();
  const pick = open[(next + open.length) % open.length]!;
  await router.replace(to(pick.name));
  await nextTick();
  list.value?.querySelector<HTMLElement>(`[id="${tabId(pick.name)}"]`)?.focus();
}

/** On a narrow screen the tabs scroll: keep the chosen one in view. */
async function reveal() {
  await nextTick();
  list.value
    ?.querySelector<HTMLElement>('[aria-selected="true"]')
    ?.scrollIntoView({ block: "nearest", inline: "nearest" });
}
watch(() => props.stage, reveal);
onMounted(reveal);
</script>

<template>
  <div ref="list" class="tabs" role="tablist" aria-label="Stages">
    <template v-for="s in stages" :key="s.name">
      <RouterLink v-if="s.state !== 'upcoming'" v-slot="{ href, navigate }" custom :to="to(s.name)" replace>
        <a
          :id="tabId(s.name)"
          :href="href"
          :class="['tab', `tab--${s.state}`]"
          role="tab"
          :aria-selected="s.name === stage"
          :aria-controls="panel"
          :tabindex="s.name === stage ? 0 : -1"
          @click="navigate"
          @keydown="keys"
        >
          <span class="kick">
            <span class="kick-name">{{ s.label }}</span>
            <PhCheck v-if="s.state === 'done'" class="mark" weight="bold" aria-hidden="true" />
            <PhX v-else-if="s.state === 'error'" class="mark" weight="bold" aria-hidden="true" />
            <PhMinus v-else-if="s.state === 'stopped'" class="mark" weight="bold" aria-hidden="true" />
            <span v-else class="live" aria-hidden="true"></span>
            <span class="sr-only">, {{ words[s.state] }}</span>
          </span>
          <span class="fig num">
            <span class="fig-main">{{ s.figure }}</span>
            <span v-if="s.spread" class="fig-spread">{{ s.spread }}</span>
            <span v-if="s.unit" class="fig-unit">{{ s.unit }}</span>
          </span>
        </a>
      </RouterLink>
      <div v-else :id="tabId(s.name)" class="tab tab--upcoming" role="tab" aria-selected="false" aria-disabled="true">
        <span class="kick">
          <span class="kick-name">{{ s.label }}</span>
          <span class="sr-only">, {{ words.upcoming }}</span>
        </span>
        <span class="fig num"
          ><span class="fig-main">{{ s.figure }}</span></span
        >
      </div>
    </template>
  </div>
</template>

<style scoped>
.tabs {
  background: color-mix(in oklab, var(--e-ink) 4%, var(--e-surface));
  display: grid;
  grid-auto-columns: minmax(0, 1fr);
  grid-auto-flow: column;
  border-block-end: var(--e-stroke-hair) solid var(--e-border);
  overflow-x: auto;
  scroll-snap-type: x proximity;
  scrollbar-width: none;
}

.tab {
  position: relative;
  display: grid;
  align-content: start;
  gap: var(--e-gap-snug);
  padding: var(--e-gap-loose) var(--e-pad-card);
  border-inline-end: var(--e-stroke-hair) solid var(--e-border);
  color: inherit;
  text-decoration: none;
  scroll-snap-align: start;
}

.tab:last-child {
  border-inline-end: 0;
}

a.tab:hover {
  background: var(--e-surface-tint);
}

a.tab:focus-visible {
  z-index: 1;
  outline: var(--e-stroke-focus, 2px) solid var(--e-focus, var(--e-primary));
  outline-offset: -2px;
}

/* The chosen tab shares its ground with the panel below it: the row's rule passes under it, and a brand bar caps it. */
.tab[aria-selected="true"] {
  margin-block-end: calc(-1 * var(--e-stroke-hair));
  padding-block-end: calc(var(--e-gap-loose) + var(--e-stroke-hair));
  background: var(--e-surface);
  box-shadow: inset 0 2px 0 var(--e-primary);
}

.tab--upcoming {
  color: var(--e-muted);
  cursor: default;
}

.kick {
  display: flex;
  align-items: center;
  gap: var(--e-gap-snug);
  color: var(--e-muted);
  font: var(--e-type-kicker);
  letter-spacing: var(--e-tracking-label);
  text-transform: uppercase;
}

.kick-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.mark {
  flex: none;
  width: 0.875rem;
  height: 0.875rem;
}

.live {
  flex: none;
  width: 0.5rem;
  height: 0.5rem;
  border-radius: 50%;
  background: var(--e-primary);
  animation: pulse 1.4s ease-in-out infinite;
}

.fig {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 0 var(--e-gap-snug);
  min-width: 0;
}

.fig-main {
  font: var(--e-type-numeral-l);
  letter-spacing: var(--e-tracking-tight);
}

.fig-spread,
.fig-unit {
  color: var(--e-muted);
  font: var(--e-type-numeral);
}

.fig-unit {
  font-family: var(--e-font-sans, inherit);
}

.tab--done .mark,
.tab--done .fig-main {
  color: var(--e-success);
}

.tab--error .mark,
.tab--error .fig-main {
  color: var(--e-danger);
}

.tab--current .kick,
.tab--current .fig-main {
  color: var(--e-primary);
}

.tab--stopped .fig-main {
  color: var(--e-muted);
}

.tab--upcoming .fig-main {
  color: var(--e-muted);
  opacity: 0.75;
}

.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}

@keyframes pulse {
  50% {
    opacity: 0.25;
  }
}

/* A phone: the tabs scroll sideways, each wide enough to read its figure. */
@container (width < 34rem) {
  .tabs {
    grid-auto-columns: minmax(8.5rem, 1fr);
  }

  .tab {
    padding: var(--e-gap-base) var(--e-gap-loose);
  }

  .tab[aria-selected="true"] {
    padding-block-end: calc(var(--e-gap-base) + var(--e-stroke-hair));
  }

  .fig-main {
    font-size: var(--e-text-2xl, 1.5rem);
  }
}

@media (prefers-reduced-motion: reduce) {
  .live {
    animation: none;
  }
}
</style>
