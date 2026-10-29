<script setup lang="ts">
/**
 * A row of controls: a gap of `--sp-2`, at least a control tall, wrapping by default. The default slot sits at the
 * start and `#end` at the other edge. `sticky` keeps it at the top of its scrolling pane with a hairline under it.
 * Children are `UiSeg`s, buttons and selects.
 */
withDefaults(defineProps<{ align?: "start" | "between"; wrap?: boolean; sticky?: boolean; label?: string }>(), {
  align: "start",
  wrap: true,
});
</script>

<template>
  <div
    class="ui-toolbar"
    :class="[`ui-toolbar--${align}`, { 'ui-toolbar--wrap': wrap, 'ui-toolbar--sticky': sticky }]"
    :role="label ? 'group' : undefined"
    :aria-label="label"
  >
    <slot />
    <div v-if="$slots['end']" class="ui-toolbar-end"><slot name="end" /></div>
  </div>
</template>

<style scoped>
.ui-toolbar {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  box-sizing: border-box;
  min-width: 0;
  min-height: var(--h-control);
}

.ui-toolbar--wrap {
  flex-wrap: wrap;
}

.ui-toolbar-end {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--sp-2);
  margin-inline-start: auto;
}

.ui-toolbar--sticky {
  position: sticky;
  inset-block-start: 0;
  z-index: var(--e-z-sticky);
  padding-block: var(--sp-1);
  border-block-end: var(--e-stroke-hair) solid var(--c-line);
  background: var(--c-pane);
}
</style>
