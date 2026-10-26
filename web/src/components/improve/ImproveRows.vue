<script setup lang="ts">
import { PhFlask, PhSlidersHorizontal } from "@phosphor-icons/vue";
import { RouterLink } from "vue-router";
import { EHighlight } from "@eos-ui/vue/components/EHighlight";
import { highlight, shortAgo } from "../lab/lab";
import { keyOf, pathOf, toneToken, type Item, type Status } from "./improve";

/** The library list: every test and tune in one column, the chosen one marked. */
defineProps<{
  rows: { item: Item; status: Status }[];
  /** The `keyOf` of the chosen item. */
  current: string;
  /** What the search box holds, so matches in names can be marked. */
  query: string;
}>();
</script>

<template>
  <ol class="rows">
    <li v-for="{ item, status } in rows" :key="keyOf(item)">
      <RouterLink
        class="row"
        :to="pathOf(item.kind, item.id)"
        :aria-current="keyOf(item) === current ? 'page' : undefined"
        :title="
          item.kind === 'test'
            ? `${item.test.name}: ${item.test.dev} vs ${item.test.base}`
            : `${item.tune.name}: tune of ${item.tune.ref}`
        "
      >
        <span class="mark" :style="{ background: `var(--e-${toneToken(status.tone)})` }" aria-hidden="true"></span>
        <span class="top">
          <component :is="item.kind === 'test' ? PhFlask : PhSlidersHorizontal" class="kind" aria-hidden="true" />
          <span class="name">
            <template
              v-for="(p, i) in highlight(item.kind === 'test' ? item.test.name : item.tune.name, query)"
              :key="i"
            >
              <EHighlight v-if="p.hit">{{ p.text }}</EHighlight>
              <template v-else>{{ p.text }}</template>
            </template>
          </span>
          <span class="time num">{{ shortAgo(item.at) }}</span>
        </span>
        <span class="refs num">
          <template v-if="item.kind === 'test'">{{ item.test.dev }} vs {{ item.test.base }}</template>
          <template v-else>Tune · {{ item.tune.ref }}</template>
        </span>
        <span :class="['result', `result--${status.tone}`]">
          <strong class="label">{{ status.label }}</strong>
          <span v-if="status.figure" class="num">{{ status.figure }}</span>
        </span>
      </RouterLink>
    </li>
  </ol>
</template>

<style scoped>
.rows {
  margin: 0;
  padding: 0;
  list-style: none;
}

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

.row[aria-current] .name {
  color: var(--e-ink);
}

.row:focus-visible {
  outline: 2px solid var(--e-primary);
  outline-offset: -2px;
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

.refs,
.result {
  overflow: hidden;
  color: var(--e-muted);
  font-size: var(--e-text-xs, 0.75rem);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.result {
  display: flex;
  gap: var(--e-gap-snug);
  align-items: baseline;
}

.label {
  flex: none;
  font-weight: 600;
}

.result .num {
  overflow: hidden;
  text-overflow: ellipsis;
}

.result--success {
  color: var(--e-success);
}

.result--danger {
  color: var(--e-danger);
}

.result--info {
  color: var(--e-primary);
}

.result--info::before {
  content: "";
  display: inline-block;
  width: 0.375rem;
  height: 0.375rem;
  align-self: center;
  border-radius: 50%;
  background: currentcolor;
  animation: pulse 1.4s ease-in-out infinite;
}

@keyframes pulse {
  50% {
    opacity: 0.25;
  }
}

@media (prefers-reduced-motion: reduce) {
  .result--info::before {
    animation: none;
  }
}
</style>
