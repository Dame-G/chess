<script setup lang="ts">
import { EToggletip } from "@eos-ui/vue/components/EToggletip";
import { glossary, type GlossaryKey } from "../glossary";

const props = defineProps<{
  of: GlossaryKey;
  /** Set the term in running text, without the info icon. */
  inline?: boolean;
}>();
const entry = glossary[props.of];
</script>

<template>
  <EToggletip :label="entry.term" :title="entry.term" :inline="inline">
    <template #trigger>
      <slot>{{ entry.term }}</slot>
    </template>
    {{ entry.text }}
  </EToggletip>
</template>
