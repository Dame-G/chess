<script setup lang="ts">
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { computed } from "vue";
import { confirmsOnEnter } from "./dialog";

/**
 * The one dialog frame: a single-line title, the body, and a sticky footer with Cancel at the start and the one primary
 * action at the end. Enter confirms (not inside a textarea or on a button), Escape and the close button cancel, and
 * focus lands on the first field, not on the close button. Below 40rem it is a bottom sheet. It never closes itself on
 * confirm, because the work behind it may fail: the caller sets `open` false when it is done, and may set `busy` while
 * it works. `danger` makes the confirm button red. `#footer-start` is a hint or a validation line between the buttons.
 */
const props = withDefaults(
  defineProps<{
    title: string;
    size?: "sm" | "md" | "lg";
    busy?: boolean;
    danger?: boolean;
    confirmLabel?: string;
    cancelLabel?: string;
    confirmDisabled?: boolean;
    /** A dialog that needs an answer (a confirmation): focus lands on Cancel, never on a field or the close button. */
    alert?: boolean;
    /** Something to read, not to answer: no footer, the close button is the way out. */
    plain?: boolean;
  }>(),
  { size: "md", cancelLabel: "Cancel" },
);

const emit = defineEmits<{ confirm: []; cancel: [] }>();

const open = defineModel<boolean>("open", { default: false });

const WIDTH = { sm: "26rem", md: "32rem", lg: "40rem" } as const;
const style = computed(() => ({ "--_w": WIDTH[props.size] }));

/** The first thing to type in or choose, so Enter and the keyboard start where the work is. */
const FIRST_FIELD =
  '.ui-dialog-body :is(input:not([type="hidden"], [disabled]), textarea, select, [role="radio"][tabindex="0"], [data-autofocus]), .ui-dialog-confirm:not([disabled])';

function cancel() {
  open.value = false;
  emit("cancel");
}

function confirm() {
  if (props.confirmDisabled || props.busy) return;
  emit("confirm");
}

function onKey(e: KeyboardEvent) {
  if (!confirmsOnEnter(e)) return;
  e.preventDefault();
  confirm();
}
</script>

<template>
  <EDialog
    v-model:open="open"
    class="ui-dialog"
    :title="title"
    :role="alert ? 'alertdialog' : 'dialog'"
    :initial-focus="alert ? undefined : FIRST_FIELD"
    footer="row"
    :style="style"
    @close="emit('cancel')"
    @keydown="onKey"
  >
    <div class="ui-dialog-body"><slot /></div>
    <template v-if="!plain" #actions>
      <EButton variant="quiet" class="ui-dialog-cancel" :disabled="busy" @click="cancel">{{ cancelLabel }}</EButton>
      <div class="ui-dialog-hint"><slot name="footer-start" /></div>
      <slot name="actions">
        <EButton
          v-if="confirmLabel"
          :variant="danger ? 'danger' : 'primary'"
          :loading="busy"
          :disabled="confirmDisabled"
          class="ui-dialog-confirm"
          @click="confirm"
        >
          {{ confirmLabel }}
        </EButton>
      </slot>
    </template>
  </EDialog>
</template>

<!-- The dialog is portaled out of this component, so its frame is styled by class, not scoped. -->
<style>
.ui-dialog {
  gap: 0;
}

.ui-dialog .e-dialog-head {
  padding: var(--sp-3) var(--sp-3) var(--sp-3) var(--sp-4);
}

.ui-dialog .e-dialog-title {
  min-width: 0;
  overflow: hidden;
  font: var(--t-title);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ui-dialog-body {
  padding: var(--sp-1) var(--sp-4) var(--sp-4);
  overflow: auto;
  font: var(--t-body);
}

.ui-dialog .e-dialog-body {
  padding: 0;
}

.ui-dialog .e-dialog-foot {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  padding: var(--sp-3) var(--sp-4);
  border-block-start: var(--e-stroke-hair) solid var(--c-line);
  background: var(--c-raise);
}

/* The disabled primary is a quiet, filled-in-grey button, not eos's dashed outline (issue D-6). */
.ui-dialog-confirm:disabled {
  background: var(--c-sunk);
  color: var(--c-ink-3);
}

.ui-dialog-confirm:disabled::before {
  display: none;
}

.ui-dialog-hint {
  flex: 1 1 auto;
  min-width: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

/* Under 40rem a dialog is a bottom sheet: full width, at most 85% of the screen, with a handle. */
@media (width < 40rem) {
  .ui-dialog.ui-dialog {
    inset: auto 0 0;
    width: 100%;
    max-width: 100%;
    max-height: 85dvh;
    margin: 0;
    border-end-end-radius: 0;
    border-end-start-radius: 0;
  }

  .ui-dialog .e-dialog-head {
    position: relative;
    padding-block-start: var(--sp-4);
  }

  .ui-dialog .e-dialog-head::before {
    content: "";
    position: absolute;
    inset: var(--sp-2) 50% auto auto;
    width: 2.5rem;
    height: var(--e-stroke-focus);
    margin-inline-end: -1.25rem;
    border-radius: var(--r-pill);
    background: var(--c-line-strong);
  }
}
</style>
