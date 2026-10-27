<script setup lang="ts">
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDialog } from "@eos-ui/vue/components/EDialog";

/** Asks before something that cannot be taken back quickly: one sentence of what happens, the action, and Cancel. */
defineProps<{
  title: string;
  description?: string;
  confirm: string;
  busy?: boolean;
  /** The action is a loss (logging out, disconnecting): drawn as a danger button. */
  danger?: boolean;
}>();
const emit = defineEmits<{ confirm: [] }>();
const open = defineModel<boolean>("open", { required: true });
</script>

<template>
  <EDialog v-model:open="open" :title="title" :description="description" size="sm" role="alertdialog">
    <slot />
    <template #actions>
      <EButton :variant="danger ? 'danger' : 'primary'" :loading="busy" @click="emit('confirm')">{{ confirm }}</EButton>
      <EButton variant="outline" @click="open = false">Cancel</EButton>
    </template>
  </EDialog>
</template>
