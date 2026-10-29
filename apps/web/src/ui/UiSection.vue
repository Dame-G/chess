<script setup lang="ts">
import { PhCaretDown } from "@phosphor-icons/vue";
import { useId } from "vue";

/**
 * The one section frame. Level 1 is a part of a sheet: a hairline above it, the title in `--t-head`, the content
 * `--sp-2` below. Level 2 is a part of a part: no hairline, a quieter title. `#aside` is the header's right side
 * (filters, a link, a button) and wraps under the title in a narrow container; `#note` is one muted line under the
 * title. `collapsible` turns the title into a disclosure; `v-model:open` keeps the state where the caller can see it.
 */
const props = withDefaults(
  defineProps<{
    title: string;
    level?: 1 | 2;
    collapsible?: boolean;
    id?: string;
    /** The heading element's level; defaults to h2 for level 1 and h3 for level 2. */
    headingLevel?: 2 | 3 | 4;
  }>(),
  { level: 1 },
);

const open = defineModel<boolean>("open", { default: true });

const uid = useId();
const titleId = `${props.id ?? uid}-title`;
const bodyId = `${props.id ?? uid}-body`;
</script>

<template>
  <section :id="id" class="ui-section" :class="`ui-section--${level}`" :aria-labelledby="titleId">
    <header class="ui-section-head">
      <component :is="`h${headingLevel ?? (level === 1 ? 2 : 3)}`" :id="titleId" class="ui-section-title">
        <button
          v-if="collapsible"
          type="button"
          class="ui-section-toggle"
          :aria-expanded="open"
          :aria-controls="bodyId"
          @click="open = !open"
        >
          <PhCaretDown class="ui-section-caret" :class="{ 'is-shut': !open }" aria-hidden="true" />
          {{ title }}
        </button>
        <template v-else>{{ title }}</template>
      </component>
      <div v-if="$slots['aside']" class="ui-section-aside"><slot name="aside" /></div>
      <p v-if="$slots['note']" class="ui-section-note"><slot name="note" /></p>
    </header>
    <div v-show="!collapsible || open" :id="bodyId" class="ui-section-body"><slot /></div>
  </section>
</template>

<style scoped>
.ui-section {
  container-type: inline-size;
  display: grid;
  gap: var(--sp-2);
  min-width: 0;
}

/* A part of a sheet: a hairline above, space above that, a little below the line. The first one starts the sheet. */
.ui-section--1 {
  margin-block-start: var(--sp-5);
  padding-block-start: var(--sp-3);
  border-block-start: var(--e-stroke-hair) solid var(--c-line);
}

.ui-section--2 {
  margin-block-start: var(--sp-4);
}

.ui-section:first-child {
  margin-block-start: 0;
  padding-block-start: 0;
  border-block-start: 0;
}

.ui-section-head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0 var(--sp-3);
  min-width: 0;
}

.ui-section-title {
  flex: 1 1 auto;
  min-width: 0;
  margin: 0;
  color: var(--c-ink);
  font: var(--t-head);
}

.ui-section--2 .ui-section-title {
  font: var(--t-body-strong);
}

.ui-section-aside {
  display: flex;
  flex: 0 1 auto;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--sp-2);
  margin-inline-start: auto;
  min-width: 0;
}

@container (width < 30rem) {
  .ui-section-aside {
    flex: 1 1 100%;
    margin-block-start: var(--sp-1);
    margin-inline-start: 0;
  }
}

.ui-section-note {
  flex: 1 1 100%;
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

/* Blocks inside a section sit one gap apart; a section does not have to be a grid for its children to be spaced. */
.ui-section-body {
  display: grid;
  gap: var(--sp-4);
  min-width: 0;
}

.ui-section-toggle {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-1);
  min-height: var(--h-control);
  margin-inline-start: calc(-1 * var(--sp-1));
  padding: 0 var(--sp-1);
  border: 0;
  border-radius: var(--r-1);
  background: transparent;
  color: inherit;
  font: inherit;
  cursor: pointer;
}

.ui-section-toggle:hover {
  background: var(--c-raise);
}

.ui-section-toggle:focus-visible {
  outline: var(--focus-ring);
}

.ui-section-caret {
  flex: none;
  color: var(--c-ink-2);
  font-size: var(--i-sm);
  transition: transform var(--m-fast) var(--m-ease);
}

.ui-section-caret.is-shut {
  transform: rotate(-90deg);
}

@media (prefers-reduced-motion: reduce) {
  .ui-section-caret {
    transition: none;
  }
}
</style>
