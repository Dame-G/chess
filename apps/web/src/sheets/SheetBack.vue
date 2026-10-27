<script setup lang="ts">
import { PhArrowLeft } from "@phosphor-icons/vue";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";

/** The sheet's Back button, shown in its `leading` slot when the page before was another sheet. */
defineProps<{ show: boolean }>();
defineEmits<{ back: [] }>();
</script>

<template>
  <EIconButton v-if="show" :icon="PhArrowLeft" label="Back" tooltip size="sm" @click="$emit('back')" />
</template>
