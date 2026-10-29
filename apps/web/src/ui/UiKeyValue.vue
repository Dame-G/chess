<script setup lang="ts">
import { provide } from "vue";
import { kvInline } from "./kv";
import UiKv from "./UiKv.vue";

export interface KvItem {
  /** Names the `#value-<key>` slot; defaults to the label. */
  key?: string;
  label: string;
  value: string | number;
  mono?: boolean;
  copy?: boolean;
  tone?: "win" | "loss" | "warn" | "info";
}

/**
 * Facts as a list: a muted label, the value beside it, a hairline between rows. Pass `items`, or write the rows as
 * `<UiKv label="Cores">15</UiKv>`. `columns="2"` sets two columns where there is room; `dense` makes rows table-height;
 * `inline` is one muted line of values joined by middle dots ("5+3 · local game"), their labels kept for screen readers.
 */
const props = withDefaults(defineProps<{ items?: KvItem[]; columns?: 1 | 2; dense?: boolean; inline?: boolean }>(), {
  columns: 1,
});

provide(kvInline, !!props.inline);
</script>

<template>
  <dl class="ui-kvs" :class="[`ui-kvs--${columns}`, { 'ui-kvs--dense': dense, 'ui-kvs--inline': inline }]">
    <slot>
      <UiKv
        v-for="item in items"
        :key="item.key ?? item.label"
        :label="item.label"
        :mono="item.mono"
        :copy="item.copy"
        :tone="item.tone"
      >
        <slot :name="`value-${item.key ?? item.label}`" :item="item">{{ item.value }}</slot>
      </UiKv>
    </slot>
  </dl>
</template>

<style scoped>
.ui-kvs {
  container-type: inline-size;
  display: grid;
  margin: 0;
  min-width: 0;
}

.ui-kvs--2 {
  grid-template-columns: minmax(0, 1fr);
  column-gap: var(--sp-5);
}

@container (width >= 40rem) {
  .ui-kvs--2 {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

.ui-kvs--dense :deep(.ui-kv) {
  min-height: var(--h-table-row);
}

.ui-kvs--inline {
  display: block;
  container-type: normal;
  color: var(--c-ink-2);
  font: var(--t-meta);
}
</style>
