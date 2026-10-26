<script setup lang="ts">
import { EToggletip } from "@eos-ui/vue/components/EToggletip";
import { GLOSSARY, type TermKey } from "./glossary";

/** A word of jargon with its plain meaning one tap away. `text` is what shows; it defaults to the glossary's term. */
defineProps<{ k: TermKey; text?: string }>();
</script>

<template>
  <EToggletip inline :label="GLOSSARY[k].term" :title="GLOSSARY[k].term">
    <template #trigger>{{ text ?? GLOSSARY[k].term }}</template>
    {{ GLOSSARY[k].text }}
  </EToggletip>
</template>
