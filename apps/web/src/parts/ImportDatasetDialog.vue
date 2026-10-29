<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useToast } from "@eos-ui/vue";
import { ENumberField } from "@eos-ui/vue/components/ENumberField";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import { bytes } from "../format";
import { importNeed, nameFromUrl, validName, whyNoRoom } from "../sheets/training";
import { useTraining } from "../stores/training";
import UiDialog from "../ui/UiDialog.vue";
import UiField from "../ui/UiField.vue";

/**
 * Import a dataset: one shard from a URL, downloaded and converted in the background. Nothing is downloaded without an
 * explicit go-ahead that states the size and where it comes from: the primary button is that go-ahead and carries the
 * size (`Download 7.9 GB`), under a line that names the host. The size is the one the source reports, and the download
 * stops if the server sends anything else.
 */
const open = defineModel<boolean>("open", { required: true });
const training = useTraining();
const toast = useToast();

/** What the address field shows while empty. */
const scheme = "https://";
const address = ref("");
const name = ref("");
const gigabytes = ref<number | null>(null);
const sha = ref("");
const busy = ref(false);
let named = false;

watch(open, (isOpen) => {
  if (!isOpen) return;
  address.value = "";
  name.value = "";
  gigabytes.value = null;
  sha.value = "";
  named = false;
});
// A name from the address, until it is typed over.
watch(address, (u) => {
  if (!named) name.value = nameFromUrl(u);
});

const expected = computed(() => Math.round((gigabytes.value ?? 0) * 1e9));
const need = computed(() => importNeed(expected.value));
const noRoom = computed(() => (training.data && expected.value ? whyNoRoom(need.value.peak, training.data) : null));
const host = computed(() => {
  try {
    return new URL(address.value.trim()).host;
  } catch {
    return "";
  }
});
const urlError = computed(() =>
  address.value && !/^https:\/\/\S+$/.test(address.value.trim())
    ? "Start with https://, as in https://example.com/shard.zst."
    : undefined,
);
const nameError = computed(() =>
  name.value && !validName(name.value) ? "Use letters, digits, dashes and underscores." : undefined,
);
const shaError = computed(() =>
  sha.value && !/^[0-9a-fA-F]{64}$/.test(sha.value.trim()) ? "A SHA-256 is 64 hexadecimal digits." : undefined,
);
const sizeError = computed(() => noRoom.value ?? undefined);
const ready = computed(
  () =>
    !!address.value.trim() &&
    !urlError.value &&
    validName(name.value) &&
    expected.value > 0 &&
    !shaError.value &&
    !noRoom.value,
);

async function start() {
  if (!ready.value) return;
  busy.value = true;
  try {
    const w = await training.importDataset({
      url: address.value.trim(),
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
  <UiDialog
    v-model:open="open"
    title="Import a dataset"
    size="sm"
    :confirm-label="expected ? `Download ${bytes(expected)}` : 'Download'"
    :busy="busy"
    :confirm-disabled="!ready"
    @confirm="start"
  >
    <div class="stack">
      <UiField v-slot="{ id, describedby, invalid }" label="Address" :error="urlError">
        <ETextField
          :id="id"
          v-model="address"
          label="Address"
          hide-label
          :placeholder="scheme"
          autocomplete="off"
          :aria-describedby="describedby"
          :aria-invalid="invalid || undefined"
        />
      </UiField>
      <UiField
        v-slot="{ id, describedby, invalid }"
        label="Size it reports"
        :error="sizeError"
        hint="Read it from the page that offers the file. The download stops if the server sends a different size."
      >
        <ENumberField
          :id="id"
          v-model="gigabytes"
          label="Size it reports"
          hide-label
          suffix="GB"
          class="num"
          :min="0"
          :step="0.1"
          :aria-describedby="describedby"
          :aria-invalid="invalid || undefined"
        />
      </UiField>
      <UiField v-slot="{ id, describedby, invalid }" label="Name" :error="nameError">
        <ETextField
          :id="id"
          v-model="name"
          label="Name"
          hide-label
          autocomplete="off"
          :aria-describedby="describedby"
          :aria-invalid="invalid || undefined"
          @update:model-value="named = true"
        />
      </UiField>
      <UiField
        v-slot="{ id, describedby, invalid }"
        label="SHA-256"
        optional
        :error="shaError"
        hint="If the page gives one, the file is checked against it."
      >
        <ETextField
          :id="id"
          v-model="sha"
          label="SHA-256"
          hide-label
          autocomplete="off"
          :aria-describedby="describedby"
          :aria-invalid="invalid || undefined"
        />
      </UiField>
      <p v-if="expected && host" class="go" role="status">
        <strong>Download {{ bytes(expected) }} from {{ host }}?</strong>
        It goes into your data folder (data/{{ name || "name" }}), needs up to {{ bytes(need.peak) }} while it converts
        and keeps about {{ bytes(need.kept) }}.
      </p>
    </div>
  </UiDialog>
</template>

<style scoped>
/* A number takes the room a number needs: narrow enough that the field drops its step buttons. */
.num {
  inline-size: var(--w-field);
}

.go {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.go strong {
  display: block;
  color: var(--c-ink);
  font: var(--t-body-strong);
}
</style>
