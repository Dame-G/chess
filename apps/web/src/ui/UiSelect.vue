<script setup lang="ts" generic="T extends string">
import { ESelect } from "@eos-ui/vue/components/ESelect";
import UiField from "./UiField.vue";

interface SelectOption<V> {
  value: V;
  label: string;
  description?: string;
  disabled?: boolean;
  group?: string;
}

/**
 * The one dropdown: a list of options behind a button. By default it is a field, with its label above it (in the
 * kit's label style), then the hint or error; `dense` is the button alone, a step shorter, for a toolbar or a filter
 * bar, with the label kept for screen readers. Strings are their own labels. Use `UiSeg` for a handful of options that
 * should stay in view.
 */
defineProps<{
  label: string;
  options: readonly (T | SelectOption<T>)[];
  hint?: string;
  error?: string;
  placeholder?: string;
  optional?: boolean;
  disabled?: boolean;
  /** The button alone in a toolbar: no field frame, the label for screen readers only. */
  dense?: boolean;
}>();

const model = defineModel<T | "">();
</script>

<template>
  <ESelect
    v-if="dense"
    v-model="model"
    :label="label"
    hide-label
    size="sm"
    :options="options"
    :placeholder="placeholder"
    :disabled="disabled"
    class="ui-select ui-select--dense"
  />
  <UiField v-else v-slot="{ id, describedby, invalid }" :label="label" :hint="hint" :error="error" :optional="optional">
    <ESelect
      :id="id"
      v-model="model"
      :label="label"
      hide-label
      :options="options"
      :placeholder="placeholder"
      :disabled="disabled"
      :aria-describedby="describedby"
      :aria-invalid="invalid || undefined"
      class="ui-select"
    />
  </UiField>
</template>

<style scoped>
.ui-select--dense {
  min-width: var(--w-field);
}
</style>
