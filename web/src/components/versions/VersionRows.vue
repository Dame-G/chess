<script setup lang="ts">
import { PhCrown, PhGitBranch } from "@phosphor-icons/vue";
import { RouterLink } from "vue-router";
import { EHighlight } from "@eos-ui/vue/components/EHighlight";
import { versionPath, type Version } from "../../api/queries";
import { toneToken } from "../improve/improve";
import { highlight, shortAgo } from "../lab/lab";
import { tuneBadge, verdictStatus } from "./versions";

/** The library list: one row per version, the chosen one marked. */
defineProps<{
  versions: readonly Version[];
  /** The name of the chosen version. */
  current: string;
  /** What the search box holds, so matches in names can be marked. */
  query: string;
}>();
</script>

<template>
  <ol class="rows">
    <li v-for="v in versions" :key="v.name">
      <RouterLink
        class="row"
        :to="versionPath(v.name)"
        :aria-current="v.name === current ? 'page' : undefined"
        :class="{ 'row--upstream': v.upstream, 'row--hidden': v.hidden }"
        :title="`${v.name}: ${v.subject}`"
      >
        <span
          class="mark"
          :style="{ background: `var(--e-${toneToken(v.is_main ? 'info' : verdictStatus(v).tone)})` }"
          aria-hidden="true"
        ></span>
        <span class="top">
          <PhGitBranch class="kind" aria-hidden="true" />
          <span class="name">
            <template v-for="(p, i) in highlight(v.name, query)" :key="i">
              <EHighlight v-if="p.hit">{{ p.text }}</EHighlight>
              <template v-else>{{ p.text }}</template>
            </template>
          </span>
          <span v-if="v.is_main" class="tag">reference</span>
          <span v-else-if="v.hidden" class="tag">hidden</span>
          <PhCrown v-if="v.on_lichess" class="lichess" aria-label="Plays on Lichess" role="img" />
          <span class="time num">{{ shortAgo(v.date) }}</span>
        </span>
        <span class="subject">{{ v.subject }}</span>
        <span v-if="!v.is_main && !v.upstream" class="status">
          <span :class="['result', `result--${verdictStatus(v).tone}`]">
            <strong class="label">{{ verdictStatus(v).label }}</strong>
            <span v-if="verdictStatus(v).figure" class="num">{{ verdictStatus(v).figure }}</span>
          </span>
          <span v-if="v.tune" class="badge num">{{ tuneBadge(v) }}</span>
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

.row:focus-visible {
  outline: 2px solid var(--e-primary);
  outline-offset: -2px;
}

.row--upstream .name,
.row--hidden .name {
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
  min-width: 0;
  overflow: hidden;
  font-family: var(--e-font-mono);
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tag {
  flex: none;
  padding-inline: var(--e-gap-snug);
  background: var(--e-surface-tint);
  color: var(--e-muted);
  font-size: var(--e-text-xs, 0.75rem);
}

.lichess {
  flex: none;
  color: var(--e-primary);
}

.time {
  flex: none;
  margin-inline-start: auto;
  color: var(--e-muted);
  font-size: var(--e-text-xs, 0.75rem);
}

.subject,
.status {
  overflow: hidden;
  color: var(--e-muted);
  font-size: var(--e-text-xs, 0.75rem);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.status {
  display: flex;
  flex-wrap: wrap;
  gap: 0 var(--e-gap-base);
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

.badge {
  color: var(--e-primary);
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
