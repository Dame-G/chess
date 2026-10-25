<script setup lang="ts">
import { computed, h, nextTick, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECombobox } from "@eos-ui/vue/components/ECombobox";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import { api } from "../../api/runner";
import { unwrap, useLichessBots } from "../../api/queries";
import { speedName, speedOf } from "../../lichess";
import { stored } from "../../stored";

const props = defineProps<{ online: boolean }>();

const route = useRoute();
const router = useRouter();
const toast = useToast();
const { state: bots } = useLichessBots(() => props.online);

// Minutes and seconds, as Lichess writes a clock: 3+2 is three minutes and two seconds a move.
const clocks = [
  { value: "60,0", label: "1+0" },
  { value: "180,0", label: "3+0" },
  { value: "180,2", label: "3+2" },
  { value: "300,3", label: "5+3" },
  { value: "600,5", label: "10+5" },
];
const colours = [
  { value: "random", label: "Random" },
  { value: "white", label: "White" },
  { value: "black", label: "Black" },
];

const username = ref("");
const clock = stored("lichess.clock", "180,2");
const rated = stored("lichess.rated", false);
const color = stored("lichess.color", "random");
const busy = ref(false);
const error = ref("");

const seconds = computed(() => Number(clock.value.split(",")[0]));
const increment = computed(() => Number(clock.value.split(",")[1]));
const speed = computed(() => speedOf(seconds.value, increment.value));

/** Bots to pick from, each with the rating it has at the speed chosen. */
const options = computed(() =>
  (bots.value.data ?? []).map((b) => {
    const rating = b.ratings[speed.value];
    return {
      value: b.name,
      label: b.name,
      description: [b.title, rating ? `${speedName(speed.value)} ${rating}` : ""].filter(Boolean).join(" · "),
    };
  }),
);

const ready = computed(() => props.online && username.value.trim().length > 0);
const combobox = ref<{ focus: () => void } | null>(null);

async function send() {
  if (!ready.value || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    const sent = await unwrap(
      api.POST("/api/lichess/challenge", {
        body: {
          username: username.value.trim(),
          clock_seconds: seconds.value,
          increment: increment.value,
          rated: rated.value,
          color: color.value as "random" | "white" | "black",
        },
      }),
    );
    toast.show({
      title: "Challenge sent",
      message: `Waiting for ${sent.username} to accept.`,
      tone: "success",
      action: h(EButton, { size: "sm", variant: "outline", href: sent.url, external: true }, () => "View on Lichess"),
    });
    username.value = "";
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}

// The command palette sends `?challenge=1`: put the cursor in the opponent field.
async function focusField() {
  if (route.query.challenge !== "1") return;
  await nextTick();
  combobox.value?.focus();
  const query = { ...route.query };
  delete query.challenge;
  await router.replace({ query });
}
onMounted(focusField);
watch(() => route.query.challenge, focusField);
</script>

<template>
  <form class="form" @submit.prevent="send">
    <ECombobox
      ref="combobox"
      v-model="username"
      label="Opponent"
      size="sm"
      freeform
      autocomplete="off"
      placeholder="Pick a bot or type a username"
      :options="options"
      :loading="online && bots.status === 'pending'"
    />
    <ESegmented
      v-model="clock"
      label="Clock"
      size="sm"
      :options="clocks"
      :hint="`${speedName(speed)}, in minutes plus seconds a move.`"
    />
    <ESegmented v-model="color" label="Your bot plays" size="sm" :options="colours" />
    <ESwitch v-model="rated" label="Rated" />
    <ENotice v-if="error" tone="danger" title="The challenge was not sent" :role="false">{{ error }}</ENotice>
    <EButton type="submit" :disabled="!ready" :loading="busy">Challenge</EButton>
    <p v-if="!online" class="hint">Switch the bot online to send a challenge.</p>
  </form>
</template>

<style scoped>
.form {
  display: grid;
  justify-items: start;
  gap: var(--e-gap-base);
}

.form > :deep(.e-field),
.form > :deep(.e-segmented),
.form > :deep(fieldset) {
  inline-size: 100%;
}

.hint {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
