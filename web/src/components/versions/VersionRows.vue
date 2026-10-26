<script setup lang="ts">
import { PhCrown, PhGitBranch } from "@phosphor-icons/vue";
import { versionPath, type Version } from "../../api/queries";
import LibraryRow from "../improve/LibraryRow.vue";
import StatusLine from "../improve/StatusLine.vue";
import { shortAgo } from "../lab/lab";
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
      <LibraryRow
        :to="versionPath(v.name)"
        :current="v.name === current"
        :tone="v.is_main ? 'info' : verdictStatus(v).tone"
        :icon="PhGitBranch"
        :title="v.subject"
        :refs="v.name"
        :time="shortAgo(v.date)"
        :hint="`${v.name}: ${v.subject} (${v.sha.slice(0, 7)})`"
        :query="query"
        :muted="v.upstream || v.hidden"
      >
        <template #after>
          <span v-if="v.hidden" class="tag">hidden</span>
          <PhCrown v-if="v.on_lichess" class="lichess" aria-label="Plays on Lichess" role="img" />
        </template>
        <StatusLine v-if="v.is_main" :status="{ label: 'Baseline', figure: '', tone: 'neutral' }" />
        <StatusLine v-else-if="v.upstream" :status="{ label: 'Upstream', figure: '', tone: 'neutral' }" />
        <span v-else class="status">
          <StatusLine :status="verdictStatus(v)" />
          <span v-if="v.tune" class="badge num">{{ tuneBadge(v) }}</span>
        </span>
      </LibraryRow>
    </li>
  </ol>
</template>

<style scoped>
.rows {
  margin: 0;
  padding: 0;
  list-style: none;
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

.status {
  display: flex;
  flex-wrap: wrap;
  gap: 0 var(--e-gap-base);
  overflow: hidden;
  font-size: var(--e-text-xs, 0.75rem);
  white-space: nowrap;
}

.badge {
  color: var(--e-primary);
}
</style>
