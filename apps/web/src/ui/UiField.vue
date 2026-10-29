<script setup lang="ts">
import { computed, useId } from "vue";

/**
 * A form field's frame: the label (sentence case, muted, "Optional" after it), the control, then the hint or, when
 * there is one, the error in its place. The control binds what the slot hands it: `id` for its input (so the label
 * names it), `describedby` for the hint or error and `invalid`. A group of controls (a `UiSeg`) sets `group`, and the
 * label then names the group instead of pointing at one input.
 */
const props = defineProps<{
  label: string;
  hint?: string;
  error?: string;
  optional?: boolean;
  /** An id for the control; otherwise one is made. */
  for?: string;
  group?: boolean;
}>();

const uid = useId();
const id = computed(() => props.for ?? `${uid}-control`);
const noteId = computed(() => `${id.value}-note`);
const note = computed(() => props.error ?? props.hint);
const describedby = computed(() => (note.value ? noteId.value : undefined));
</script>

<template>
  <div class="ui-field" :role="group ? 'group' : undefined" :aria-labelledby="group ? `${id}-label` : undefined">
    <component :is="group ? 'span' : 'label'" :id="`${id}-label`" class="ui-field-label" :for="group ? undefined : id">
      {{ label }}<span v-if="optional" class="ui-field-optional">Optional</span>
    </component>
    <slot :id="id" :describedby="describedby" :invalid="!!error" />
    <p
      v-if="note"
      :id="noteId"
      class="ui-field-note"
      :class="{ 'is-error': error }"
      :aria-live="error ? 'polite' : undefined"
    >
      {{ note }}
    </p>
  </div>
</template>

<style scoped>
.ui-field {
  display: grid;
  gap: var(--sp-1);
  min-width: 0;
}

.ui-field-label {
  color: var(--c-ink-2);
  font: 500 var(--e-text-xs) / 1.35 var(--e-font-sans);
}

.ui-field-optional {
  margin-inline-start: var(--sp-1);
  font-weight: 400;
}

.ui-field-optional::before {
  content: "· ";
}

.ui-field-note {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.ui-field-note.is-error {
  color: var(--c-loss);
}
</style>
