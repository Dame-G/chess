<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECheckbox } from "@eos-ui/vue/components/ECheckbox";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ENumberField } from "@eos-ui/vue/components/ENumberField";
import { EForm } from "@eos-ui/vue/components/EForm";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import { bytes } from "../format";
import { importConfirmText, importNeed, nameFromUrl, validName, whyNoRoom } from "../sheets/training";
import { useTraining } from "../stores/training";

/**
 * Import a dataset: one shard from a URL, downloaded and converted in the background. Nothing is downloaded without an
 * explicit go-ahead that states the size and where it goes; the size is the one the source reports, and the download
 * stops if the server sends anything else.
 */
const open = defineModel<boolean>("open", { required: true });
const training = useTraining();
const toast = useToast();

const url = ref("");
const name = ref("");
const gigabytes = ref<number | null>(null);
const sha = ref("");
const agreed = ref(false);
const busy = ref(false);
let named = false;

watch(open, (isOpen) => {
  if (!isOpen) return;
  url.value = "";
  name.value = "";
  gigabytes.value = null;
  sha.value = "";
  agreed.value = false;
  named = false;
});
// A name from the address, until it is typed over.
watch(url, (u) => {
  if (!named) name.value = nameFromUrl(u);
});

const expected = computed(() => Math.round((gigabytes.value ?? 0) * 1e9));
const need = computed(() => importNeed(expected.value));
const noRoom = computed(() => (training.data && expected.value ? whyNoRoom(need.value.peak, training.data) : null));
const urlError = computed(() =>
  url.value && !/^https:\/\/\S+$/.test(url.value.trim())
    ? "Start with https://, as in https://example.com/shard.zst."
    : undefined,
);
const nameError = computed(() =>
  name.value && !validName(name.value) ? "Use letters, digits, dashes and underscores." : undefined,
);
const shaError = computed(() =>
  sha.value && !/^[0-9a-fA-F]{64}$/.test(sha.value.trim()) ? "A SHA-256 is 64 hexadecimal digits." : undefined,
);
const ready = computed(
  () =>
    !!url.value.trim() &&
    !urlError.value &&
    validName(name.value) &&
    expected.value > 0 &&
    !shaError.value &&
    !noRoom.value,
);

async function start() {
  busy.value = true;
  try {
    const w = await training.importDataset({
      url: url.value.trim(),
      name: name.value.trim(),
      expected_bytes: expected.value,
      ...(sha.value.trim() ? { sha256: sha.value.trim().toLowerCase() } : {}),
      confirm: true,
    });
    open.value = false;
    toast.show({ title: "Importing the dataset", message: w.title, tone: "success" });
  } catch (e) {
    toast.show({
      title: "The import did not start",
      message: e instanceof Error ? e.message : String(e),
      tone: "warning",
    });
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <EDialog
    v-model:open="open"
    title="Import a dataset"
    description="One shard from the web, downloaded and converted to the engine's format."
    size="md"
    initial-focus="#import-form input"
  >
    <EForm id="import-form" @submit.prevent="start">
      <ETextField v-model="url" label="Address" placeholder="https://" autocomplete="off" :error="urlError" />
      <ENumberField
        v-model="gigabytes"
        label="Size it reports"
        suffix="GB"
        :min="0"
        :step="0.1"
        hint="Read it from the page that offers the file. The download stops if the server sends a different size."
      />
      <ETextField
        v-model="name"
        label="Name"
        autocomplete="off"
        :error="nameError"
        @update:model-value="named = true"
      />
      <ETextField
        v-model="sha"
        label="SHA-256"
        optional
        autocomplete="off"
        :error="shaError"
        hint="If the page gives one, the file is checked against it."
      />
      <template v-if="expected">
        <p class="sec-note">{{ importConfirmText(name, expected) }}</p>
        <ENotice v-if="noRoom" tone="warning" title="There is not enough room">{{ noRoom }}</ENotice>
        <ECheckbox
          v-model="agreed"
          :label="`Yes, download ${bytes(expected)}`"
          description="Nothing is downloaded until this is ticked."
        />
      </template>
    </EForm>
    <template #actions>
      <EButton type="submit" form="import-form" :loading="busy" :disabled="!ready || !agreed">Import</EButton>
      <EButton variant="outline" @click="open = false">Cancel</EButton>
    </template>
  </EDialog>
</template>
