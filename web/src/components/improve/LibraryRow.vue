<script setup lang="ts">
import type { Component } from "vue";
import { RouterLink, type RouteLocationRaw } from "vue-router";
import { EHighlight } from "@eos-ui/vue/components/EHighlight";
import { highlight } from "../lab/lab";
import { toneToken, type Tone } from "./improve";

/**
 * One row of a library, the same in Improve and Versions: a title in sans, the ref it is about in mono, then the status
 * line. The colour mark at the edge is the verdict; the chosen row is `aria-current`.
 */
defineProps<{
  to: RouteLocationRaw;
  current: boolean;
  tone: Tone;
  icon: Component;
  title: string;
  /** The branch or build the title is about, in mono. */
  refs: string;
  /** A short age, "3h". */
  time: string;
  /** What shows on hover: the full name and the commit. */
  hint: string;
  /** What the search box holds, so matches are marked. */
  query: string;
  muted?: boolean;
}>();
</script>

<template>
  <RouterLink
    :class="['row', { 'row--muted': muted }]"
    :to="to"
    :aria-current="current ? 'page' : undefined"
    :title="hint"
  >
    <span class="mark" :style="{ background: `var(--e-${toneToken(tone)})` }" aria-hidden="true"></span>
    <span class="top">
      <component :is="icon" class="kind" aria-hidden="true" />
      <span class="name">
        <template v-for="(p, i) in highlight(title, query)" :key="i">
          <EHighlight v-if="p.hit">{{ p.text }}</EHighlight>
          <template v-else>{{ p.text }}</template>
        </template>
      </span>
      <slot name="after" />
      <span class="time num">{{ time }}</span>
    </span>
    <span class="refs num">
      <template v-for="(p, i) in highlight(refs, query)" :key="i">
        <EHighlight v-if="p.hit">{{ p.text }}</EHighlight>
        <template v-else>{{ p.text }}</template>
      </template>
    </span>
    <slot />
  </RouterLink>
</template>

<style scoped>
.row {
  position: relative;
  display: grid;
  gap: var(--e-gap-tight);
  padding: var(--e-gap-base) var(--ws-pad, var(--e-pad-control-sm));
  padding-inline-start: calc(var(--ws-pad, var(--e-pad-control-sm)) + 0.25rem);
  border-block-end: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 35%, transparent);
  color: inherit;
  text-decoration: none;
}

.row:hover {
  background: var(--e-surface-tint);
}

.row[aria-current] {
  background: var(--e-primary-soft);
}

.row[aria-current] .mark {
  width: 4px;
}

.row:focus-visible {
  outline: 2px solid var(--e-primary);
  outline-offset: -2px;
}

.row--muted .name {
  color: var(--e-muted);
}

.mark {
  position: absolute;
  inset-block: 0;
  inset-inline-start: 0;
  width: 3px;
}

.top {
  display: flex;
  align-items: center;
  gap: var(--e-gap-snug);
}

.kind {
  flex: none;
  color: var(--e-muted);
}

.name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.time {
  flex: none;
  color: var(--e-muted);
  font-size: var(--e-text-xs, 0.75rem);
}

.refs {
  overflow: hidden;
  color: var(--e-muted);
  font-size: var(--e-text-xs, 0.75rem);
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
