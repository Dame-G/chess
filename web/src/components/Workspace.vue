<script setup lang="ts">
/**
 * The frame of every workspace: a library on the left (what you pick from), the stage in the middle (what you look
 * at) and an inspector on the right (what you read about it). Panes are flat, divided by hairlines, and scroll on their
 * own, so the stage never moves. Below 64rem they stack and the page scrolls instead.
 */
defineProps<{
  /** The library's accessible name, such as "Experiments". */
  libraryLabel?: string;
  /** The inspector's accessible name. */
  inspectorLabel?: string;
}>();
defineSlots<{
  /** A short bar above the library list: its title, a filter, the one action. */
  "library-head"?(): unknown;
  library?(): unknown;
  /** A bar above the stage, such as a transport or a title. */
  "stage-head"?(): unknown;
  default(): unknown;
  /** A bar under the stage: transport controls, a timeline. */
  "stage-foot"?(): unknown;
  "inspector-head"?(): unknown;
  inspector?(): unknown;
}>();
</script>

<template>
  <div :class="['ws', { 'ws--library': $slots.library, 'ws--inspector': $slots.inspector }]">
    <aside v-if="$slots.library" class="ws-pane ws-library" :aria-label="libraryLabel">
      <div v-if="$slots['library-head']" class="ws-head"><slot name="library-head" /></div>
      <div class="ws-scroll"><slot name="library" /></div>
    </aside>

    <section class="ws-stage">
      <div v-if="$slots['stage-head']" class="ws-head"><slot name="stage-head" /></div>
      <div class="ws-scroll ws-stage-body"><slot /></div>
      <div v-if="$slots['stage-foot']" class="ws-foot"><slot name="stage-foot" /></div>
    </section>

    <aside v-if="$slots.inspector" class="ws-pane ws-inspector" :aria-label="inspectorLabel">
      <div v-if="$slots['inspector-head']" class="ws-head"><slot name="inspector-head" /></div>
      <div class="ws-scroll"><slot name="inspector" /></div>
    </aside>
  </div>
</template>

<style scoped>
.ws {
  /* The inline padding of every bar. Lists in a pane use it too, so their text lines up with the bar above. */
  --ws-pad: var(--e-pad-control);
  --ws-line: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 55%, transparent);

  display: grid;
  grid-template-columns: minmax(0, 1fr);
  height: 100%;
  min-height: 0;
}

.ws--library {
  grid-template-columns: var(--ws-library, 18rem) minmax(0, 1fr);
}

.ws--inspector {
  grid-template-columns: minmax(0, 1fr) var(--ws-inspector, 24rem);
}

.ws--library.ws--inspector {
  grid-template-columns: var(--ws-library, 18rem) minmax(0, 1fr) var(--ws-inspector, 24rem);
}

.ws-pane,
.ws-stage {
  display: grid;
  grid-template-rows: auto minmax(0, 1fr) auto;
  min-width: 0;
  min-height: 0;
}

.ws-pane {
  background: var(--e-surface);
}

.ws-library {
  border-inline-end: var(--ws-line);
}

.ws-inspector {
  border-inline-start: var(--ws-line);
}

.ws-head,
.ws-foot {
  grid-row: 1;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--e-gap-snug) var(--e-gap-base);
  min-height: calc(var(--e-control-height) + var(--e-gap-base));
  padding: var(--e-gap-snug) var(--ws-pad);
  box-sizing: border-box;
}

.ws-head {
  border-block-end: var(--ws-line);
}

.ws-foot {
  grid-row: 3;
  border-block-start: var(--ws-line);
  background: var(--e-surface);
}

/* Each part keeps its row whether or not the bars around it exist, so the body always takes the free height. */
.ws-scroll {
  grid-row: 2;
  min-height: 0;
  overflow: auto;
  overscroll-behavior: contain;
}

.ws-stage-body {
  display: grid;
}

@media (width < 64rem) {
  .ws,
  .ws--library,
  .ws--inspector,
  .ws--library.ws--inspector {
    grid-template-columns: minmax(0, 1fr);
    height: auto;
  }

  .ws-library {
    order: 2;
    border-inline-end: 0;
    border-block-start: var(--ws-line);
  }

  .ws-inspector {
    order: 1;
    border-inline-start: 0;
    border-block-start: var(--ws-line);
  }

  .ws-scroll {
    overflow: visible;
  }
}
</style>
