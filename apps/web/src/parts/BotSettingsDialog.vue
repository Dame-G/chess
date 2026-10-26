<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECheckboxGroup } from "@eos-ui/vue/components/ECheckboxGroup";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { EDisclosure } from "@eos-ui/vue/components/EDisclosure";
import { ENumberField } from "@eos-ui/vue/components/ENumberField";
import { ESelect } from "@eos-ui/vue/components/ESelect";
import { EStack } from "@eos-ui/vue/components/EStack";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import type { BotSettings, Speed } from "../api/types";
import { speedName } from "../format";
import { useConnections } from "../stores/connections";
import { useVersions } from "../stores/versions";

/**
 * The bot's rules: which version plays, which speeds and clocks and challengers it takes, how many games at once, and
 * the clocks it looks for games at. Clocks are written in minutes here and kept in seconds, as the settings are.
 */
const props = defineProps<{ settings: BotSettings }>();
const open = defineModel<boolean>("open", { required: true });

const connections = useConnections();
const versions = useVersions();
const toast = useToast();

const draft = ref<BotSettings>({ ...props.settings });
const seekText = ref("");
const busy = ref(false);

watch(open, (isOpen) => {
  if (!isOpen) return;
  draft.value = { ...props.settings, speeds: [...props.settings.speeds] };
  seekText.value = props.settings.seek_clocks.join(", ");
});

const speedOptions: { value: Speed; label: string }[] = (["bullet", "blitz", "rapid", "classical"] as const).map(
  (s) => ({
    value: s,
    label: speedName[s],
  }),
);
const versionOptions = computed(() => [
  { value: "champion", label: "The champion at the start of each game" },
  ...[...versions.line].reverse().map((v) => ({ value: v.name, label: v.name })),
]);

const version = computed({
  get: () => draft.value.version ?? "champion",
  set: (v: string | undefined) => (draft.value.version = v || "champion"),
});

/** Minutes in the field, seconds in the settings. */
const minutes = (key: "min_initial" | "max_initial") =>
  computed({
    get: () => draft.value[key] / 60,
    set: (v: number | null) => (draft.value[key] = Math.round((v ?? 0) * 60)),
  });
const minInitial = minutes("min_initial");
const maxInitial = minutes("max_initial");

async function save() {
  busy.value = true;
  try {
    const seek_clocks = seekText.value
      .split(/[,\s]+/)
      .map((c) => c.trim())
      .filter(Boolean);
    await connections.saveBotSettings({ ...draft.value, seek_clocks });
    open.value = false;
    toast.show({ title: "The bot's rules are saved", tone: "success" });
  } catch (e) {
    toast.show({
      title: "The rules were not saved",
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
    title="The bot's rules"
    description="What Dameless plays on Lichess, and which challenges it takes."
    size="md"
  >
    <EStack gap="base">
      <ESelect v-model="version" label="Version that plays" :options="versionOptions" />
      <ECheckboxGroup
        v-model="draft.speeds"
        label="Speeds"
        orientation="horizontal"
        :options="speedOptions"
        hint="Correspondence and UltraBullet are never played."
      />
      <div class="pair">
        <ESwitch v-model="draft.accept_rated" label="Rated games" />
        <ESwitch v-model="draft.accept_casual" label="Casual games" />
        <ESwitch v-model="draft.accept_humans" label="From people" />
        <ESwitch v-model="draft.accept_bots" label="From bots" />
      </div>
      <div class="pair">
        <ENumberField v-model="minInitial" label="Shortest game" suffix="min" :min="0" :max="180" :step="0.5" />
        <ENumberField v-model="maxInitial" label="Longest game" suffix="min" :min="0" :max="180" :step="1" />
        <ENumberField v-model="draft.min_increment" label="Smallest added time" suffix="s" :min="0" :max="180" />
        <ENumberField v-model="draft.max_increment" label="Largest added time" suffix="s" :min="0" :max="180" />
      </div>
      <ENumberField v-model="draft.max_games" label="Games at once" :min="1" :max="16" />
      <ETextField
        v-model="seekText"
        label="Clocks to look for games at"
        hint="Minutes plus seconds, separated by commas: 3+0, 3+2, 5+3."
        autocomplete="off"
      />
      <EDisclosure title="Engine">
        <div class="pair">
          <ENumberField v-model="draft.hash" label="Hash" suffix="MiB" :min="1" :max="4096" />
          <ENumberField v-model="draft.threads" label="Threads" :min="1" :max="64" />
          <ENumberField
            v-model="draft.move_overhead"
            label="Move overhead"
            suffix="ms"
            :min="0"
            :max="2000"
            :step="50"
          />
        </div>
      </EDisclosure>
    </EStack>
    <template #actions>
      <EButton :loading="busy" @click="save">Save rules</EButton>
      <EButton variant="outline" @click="open = false">Cancel</EButton>
    </template>
  </EDialog>
</template>

<style scoped>
.pair {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--e-gap-snug) var(--e-gap-base);
}
</style>
