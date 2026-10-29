<script setup lang="ts">
import { computed } from "vue";
import { blocks, type Span } from "./markdown";

/** An idea's notes: markdown drawn as plain elements, never as HTML. */
const props = defineProps<{ text: string }>();
const parsed = computed(() => blocks(props.text));
const tag = (s: Span) =>
  s.kind === "bold" ? "strong" : s.kind === "italic" ? "em" : s.kind === "code" ? "code" : "span";
</script>

<template>
  <div class="notes">
    <template v-for="(b, i) in parsed" :key="i">
      <p v-if="b.type === 'p'">
        <component :is="tag(s)" v-for="(s, j) in b.spans" :key="j">{{ s.text }}</component>
      </p>
      <h4 v-else-if="b.type === 'h'">
        <component :is="tag(s)" v-for="(s, j) in b.spans" :key="j">{{ s.text }}</component>
      </h4>
      <component :is="b.type" v-else>
        <li v-for="(item, k) in b.items" :key="k">
          <component :is="tag(s)" v-for="(s, j) in item" :key="j">{{ s.text }}</component>
        </li>
      </component>
    </template>
  </div>
</template>

<style scoped>
.notes {
  display: grid;
  gap: var(--sp-2);
  min-width: 0;
  overflow-wrap: anywhere;
}

.notes :is(p, h4, ul, ol) {
  margin: 0;
}

.notes :is(ul, ol) {
  padding-inline-start: var(--sp-5);
}

.notes h4 {
  font: var(--t-body-strong);
}

.notes code {
  padding: 0 var(--sp-1);
  border-radius: var(--r-1);
  background: var(--c-sunk);
  font: var(--t-fig);
}
</style>
