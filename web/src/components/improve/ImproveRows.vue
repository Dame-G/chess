<script setup lang="ts">
import { PhFlask, PhSlidersHorizontal } from "@phosphor-icons/vue";
import { shortAgo } from "../lab/lab";
import { keyOf, pathOf, type Item, type Status } from "./improve";
import LibraryRow from "./LibraryRow.vue";
import StatusLine from "./StatusLine.vue";

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
      <LibraryRow
        v-if="item.kind === 'test'"
        :to="pathOf('test', item.id)"
        :current="keyOf(item) === current"
        :tone="status.tone"
        :icon="PhFlask"
        :title="item.test.name"
        :refs="`#${item.id} · ${item.test.dev} vs ${item.test.base}`"
        :time="shortAgo(item.at)"
        :hint="`${item.test.name}: ${item.test.dev} vs ${item.test.base}`"
        :query="query"
      >
        <StatusLine :status="status" />
      </LibraryRow>
      <LibraryRow
        v-else
        :to="pathOf('tune', item.id)"
        :current="keyOf(item) === current"
        :tone="status.tone"
        :icon="PhSlidersHorizontal"
        :title="item.tune.name"
        :refs="`#${item.id} · tune of ${item.tune.ref}`"
        :time="shortAgo(item.at)"
        :hint="`${item.tune.name}: tune of ${item.tune.ref}`"
        :query="query"
      >
        <StatusLine :status="status" />
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
</style>
