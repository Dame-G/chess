<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, shallowRef, watch } from "vue";
import { answer, current, registerHost } from "./useConfirm";
import UiDialog from "./UiDialog.vue";

/**
 * The host of `useConfirm()`: mount it once, in the app's root, and every confirmation in the app is this dialog. It
 * shows one question at a time (a second waits behind the first). Cancel and Escape answer no, the action answers yes.
 */
let unregister: (() => void) | undefined;
onMounted(() => (unregister = registerHost()));
onBeforeUnmount(() => unregister?.());

const open = computed({
  get: () => current.value !== null,
  set: (value: boolean) => {
    if (!value) answer(false);
  },
});

/** The last question asked, kept while the dialog fades out so its words do not vanish before it does. */
const shown = shallowRef(current.value);
watch(current, (now) => {
  if (now) shown.value = now;
});
</script>

<template>
  <UiDialog
    v-model:open="open"
    alert
    size="sm"
    :title="shown?.title ?? ''"
    :danger="shown?.danger"
    :confirm-label="shown?.confirmLabel"
    :cancel-label="shown?.cancelLabel"
    @confirm="answer(true)"
  >
    <p v-if="shown?.body" class="ui-confirm-body">{{ shown.body }}</p>
  </UiDialog>
</template>

<style scoped>
.ui-confirm-body {
  margin: 0;
  color: var(--c-ink-2);
}
</style>
