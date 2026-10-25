<script setup lang="ts">
import { nextTick, ref, useId } from "vue";
import { RouterLink, useRoute, useRouter } from "vue-router";
import { PhCheckCircle, PhCircle, PhCircleNotch, PhXCircle } from "@phosphor-icons/vue";
import type { FunnelStage } from "./lab";

/**
 * The funnel as a row of tabs: one cell per stage, with its state, its name and how it went, joined by connectors. The
 * chosen stage lives in the URL (`?stage=`), so each cell is a link; stages not reached yet cannot be chosen.
 */
const props = defineProps<{ stages: FunnelStage[]; stage: string; panel: string }>();

const route = useRoute();
const router = useRouter();
const uid = useId();
const list = ref<HTMLElement>();

const tabId = (name: string) => `${uid}-${name}`;
const to = (name: string) => ({ query: { ...route.query, stage: name } });

const icons = { done: PhCheckCircle, error: PhXCircle, current: PhCircleNotch, upcoming: PhCircle } as const;
const words = { done: "passed", error: "failed", current: "running", upcoming: "not reached" } as const;

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
</script>

<template>
  <div class="pipe">
    <div ref="list" class="cells" role="tablist" aria-label="Stages">
      <template v-for="s in stages" :key="s.name">
        <RouterLink v-if="s.state !== 'upcoming'" v-slot="{ href, navigate }" custom :to="to(s.name)" replace>
          <a
            :id="tabId(s.name)"
            :href="href"
            :class="['cell', `cell--${s.state}`]"
            role="tab"
            :aria-selected="s.name === stage"
            :aria-controls="panel"
            :tabindex="s.name === stage ? 0 : -1"
            @click="navigate"
            @keydown="keys"
          >
            <component :is="icons[s.state]" class="cell-icon" weight="fill" aria-hidden="true" />
            <span class="cell-name"
              >{{ s.label }}<span class="sr-only">, {{ words[s.state] }}</span></span
            >
            <span class="cell-line num">{{ s.line }}</span>
          </a>
        </RouterLink>
        <div
          v-else
          :id="tabId(s.name)"
          class="cell cell--upcoming"
          role="tab"
          aria-selected="false"
          aria-disabled="true"
        >
          <component :is="icons.upcoming" class="cell-icon" aria-hidden="true" />
          <span class="cell-name"
            >{{ s.label }}<span class="sr-only">, {{ words.upcoming }}</span></span
          >
          <span class="cell-line num">{{ s.line }}</span>
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.pipe {
  container-type: inline-size;
}

.cells {
  --link: var(--e-gap-loose);
  --line: color-mix(in oklab, var(--e-border) 70%, transparent);

  display: grid;
  grid-auto-columns: minmax(0, 1fr);
  grid-auto-flow: column;
  gap: var(--link);
}

.cell {
  position: relative;
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  align-items: center;
  gap: 0.125rem var(--e-gap-snug);
  padding: var(--e-gap-snug) var(--e-gap-base);
  border: var(--e-stroke-hair) solid var(--line);
  border-radius: var(--e-radius-sm);
  background: var(--e-surface);
  color: inherit;
  text-decoration: none;
}

/* The connector: a line out of each cell's end, with a small arrowhead, in the gap before the next. */
.cell:not(:last-child)::after {
  content: "";
  position: absolute;
  inset-block-start: 50%;
  inset-inline-start: 100%;
  width: var(--link);
  height: var(--e-stroke-hair);
  background: var(--line);
}

.cell:not(:last-child)::before {
  content: "";
  position: absolute;
  inset-block-start: 50%;
  inset-inline-start: calc(100% + var(--link) - 0.3125rem);
  width: 0.375rem;
  height: 0.375rem;
  border-block-start: var(--e-stroke-hair) solid var(--e-border-strong);
  border-inline-end: var(--e-stroke-hair) solid var(--e-border-strong);
  transform: translateY(-50%) rotate(45deg);
}

a.cell:hover {
  background: var(--e-surface-tint);
}

a.cell:focus-visible {
  outline: var(--e-stroke-focus, 2px) solid var(--e-focus, var(--e-primary));
  outline-offset: 2px;
}

.cell[aria-selected="true"] {
  border-color: color-mix(in oklab, var(--e-primary) 45%, var(--line));
  background: var(--e-primary-soft);
  box-shadow: inset 0 -2px 0 var(--e-primary);
}

.cell--upcoming {
  border-style: dashed;
  background: transparent;
  color: var(--e-muted);
  cursor: default;
}

.cell-icon {
  grid-row: span 2;
  width: 1.25rem;
  height: 1.25rem;
}

.cell--done .cell-icon {
  color: var(--e-success);
}

.cell--error .cell-icon {
  color: var(--e-danger);
}

.cell--current .cell-icon {
  color: var(--e-primary);
  animation: spin 1.1s linear infinite;
}

.cell--upcoming .cell-icon {
  color: var(--e-border-strong);
}

.cell-name {
  overflow: hidden;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.cell-line {
  overflow: hidden;
  color: var(--e-muted);
  font-size: var(--e-text-xs, 0.75rem);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.cell--error .cell-line {
  color: var(--e-danger);
}

.cell--current .cell-line {
  color: var(--e-primary);
}

.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

/* Too narrow for a row: the stages stack, joined by a short line under each icon. */
@container (width < 38rem) {
  .cells {
    --link: var(--e-gap-snug);

    grid-auto-flow: row;
    grid-auto-columns: auto;
  }

  .cell:not(:last-child)::before {
    display: none;
  }

  .cell:not(:last-child)::after {
    inset-block-start: 100%;
    inset-inline-start: calc(var(--e-gap-base) + 0.625rem);
    width: var(--e-stroke-hair);
    height: var(--link);
  }
}

@media (prefers-reduced-motion: reduce) {
  .cell--current .cell-icon {
    animation: none;
  }
}
</style>
