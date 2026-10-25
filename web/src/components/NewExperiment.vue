<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { ECombobox } from "@eos-ui/vue/components/ECombobox";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { EForm } from "@eos-ui/vue/components/EForm";
import { EGrid } from "@eos-ui/vue/components/EGrid";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ERadioGroup } from "@eos-ui/vue/components/ERadioGroup";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import { ETag } from "@eos-ui/vue/components/ETag";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import { ETextarea } from "@eos-ui/vue/components/ETextarea";
import { api } from "../api/runner";
import { unwrap, useBranches } from "../api/queries";
import { ago, kinds, stageLabel } from "../format";

const open = defineModel<boolean>("open", { default: false });
const router = useRouter();
const { state: branches } = useBranches();

const dev = ref("");
const base = ref("main");
const kind = ref<keyof typeof kinds>("gainer");
const name = ref("");
const hypothesis = ref("");
const ltc = ref(false);
const error = ref("");
const submitting = ref(false);

const options = computed(() =>
  (branches.value.data ?? []).map((b) => ({
    value: b.name,
    label: b.name,
    description: `${b.subject} · ${ago(b.date)}`,
  })),
);
// The newest branch that is not the base is the likely one to test.
watch(
  [open, () => branches.value.data],
  () => {
    if (open.value && !dev.value)
      dev.value =
        branches.value.data?.find((b) => b.name !== base.value && !b.name.startsWith("upstream/"))?.name ?? "";
  },
  { immediate: true },
);
const subject = computed(() => branches.value.data?.find((b) => b.name === dev.value)?.subject);

/** The stages it will go through, in words. */
const plan = computed(() =>
  (kind.value === "nonfunctional" ? ["bench", "speed"] : ["bench", "screen", "stc", ...(ltc.value ? ["ltc"] : [])]).map(
    stageLabel,
  ),
);

async function submit() {
  submitting.value = true;
  error.value = "";
  try {
    const e = await unwrap(
      api.POST("/api/experiments", {
        body: {
          dev: dev.value,
          base: base.value,
          kind: kind.value,
          name: name.value.trim() || subject.value || dev.value,
          hypothesis: hypothesis.value,
          ltc: kind.value !== "nonfunctional" && ltc.value,
        },
      }),
    );
    open.value = false;
    dev.value = "";
    name.value = "";
    hypothesis.value = "";
    await router.push(`/lab/${e.id}`);
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    submitting.value = false;
  }
}
</script>

<template>
  <EDialog
    v-model:open="open"
    title="Test an idea"
    description="Does the change on a branch make Reckless stronger? The two versions play each other until the result is clear."
    size="lg"
  >
    <EForm id="new-experiment" :submitting="submitting" @submit="submit">
      <EGrid min="14rem" gap="base">
        <ECombobox
          v-model="dev"
          label="New version (branch)"
          freeform
          :options="options"
          :loading="branches.status === 'pending'"
          hint="Commit your change to a branch first."
          required
        />
        <ECombobox
          v-model="base"
          label="Compare with"
          :options="options"
          freeform
          required
          hint="Usually main, the current best."
        />
      </EGrid>
      <ERadioGroup
        v-model="kind"
        label="What should it do?"
        :options="Object.entries(kinds).map(([value, k]) => ({ value, label: k.label, description: k.detail }))"
      />
      <ETextField
        v-model="name"
        label="Name"
        :placeholder="subject"
        optional
        hint="Left empty, it uses the commit's title."
      />
      <ETextarea
        v-model="hypothesis"
        label="Why should it work?"
        :rows="2"
        optional
        placeholder="Pruning more at shallow depth saves time for the moves that matter."
      />
      <ESwitch
        v-if="kind !== 'nonfunctional'"
        v-model="ltc"
        label="Confirm with long games too"
        hint="Adds 40-second games after the short ones: a few more hours."
      />
      <div class="plan">
        <span class="plan-label">It will go through</span>
        <template v-for="(p, i) in plan" :key="p">
          <span v-if="i" class="plan-arrow" aria-hidden="true">→</span>
          <ETag size="sm" :label="p" />
        </template>
      </div>
      <ENotice v-if="error" tone="danger" title="Could not start it" :role="false">{{ error }}</ENotice>
    </EForm>
    <template #actions>
      <EButton type="submit" form="new-experiment" :loading="submitting" :disabled="!dev.trim()">
        Start experiment
      </EButton>
      <EButton variant="quiet" @click="open = false">Cancel</EButton>
    </template>
  </EDialog>
</template>

<style scoped>
.plan {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--e-gap-tight);
  color: var(--e-muted);
  font: var(--e-type-small);
}

.plan-label {
  margin-inline-end: var(--e-gap-tight);
}
</style>
