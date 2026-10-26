<script setup lang="ts">
import { computed, h, ref } from "vue";
import { useToast } from "@eos-ui/vue";
import { useQueryCache } from "@pinia/colada";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import { api } from "../../api/runner";
import { unwrap } from "../../api/queries";
import { meClockOptions, parseClock, speedName, speedOf } from "../../lichess";
import { stored } from "../../stored";
import { colours } from "./me";

const toast = useToast();
const cache = useQueryCache();
const clocks = meClockOptions("challenge");
const who = stored<"friend" | "ai" | "bot">("lichess.me.who", "friend");
const username = ref("");
const level = stored("lichess.me.level", "3");
const clock = stored("lichess.me.challengeClock", "5+3");
const rated = stored("lichess.me.rated", true);
const color = stored("lichess.me.color", "random");
const busy = ref(false);
const error = ref("");

const targets = [
  { value: "friend", label: "A friend" },
  { value: "ai", label: "Lichess's Stockfish" },
  { value: "bot", label: "My bot" },
];
const levels = Array.from({ length: 8 }, (_, i) => ({ value: String(i + 1), label: String(i + 1) }));

const parsed = computed(() => parseClock(clock.value));
const speed = computed(() => speedName(speedOf(parsed.value.initial, parsed.value.increment)));
/** Games against the AI are always casual. */
const casualOnly = computed(() => who.value === "ai");
const ready = computed(() => who.value !== "friend" || username.value.trim().length > 0);

async function send() {
  if (!ready.value || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    const sent = await unwrap(
      api.POST("/api/lichess/me/challenge", {
        body: {
          username: who.value === "friend" ? username.value.trim() : undefined,
          ai_level: who.value === "ai" ? Number(level.value) : undefined,
          my_bot: who.value === "bot" ? true : undefined,
          clock_seconds: parsed.value.initial,
          increment: parsed.value.increment,
          rated: casualOnly.value ? false : rated.value,
          color: color.value as "random" | "white" | "black",
        },
      }),
    );
    // The status carries the new outgoing challenge; an AI game starts at once and the app opens it.
    void cache.invalidateQueries({ key: ["lichess-me"] });
    if (who.value !== "ai")
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
</script>

<template>
  <form class="form" @submit.prevent="send">
    <ESegmented v-model="who" label="Challenge" size="sm" :options="targets" />
    <ETextField
      v-if="who === 'friend'"
      v-model="username"
      label="Lichess username"
      size="sm"
      autocomplete="off"
      placeholder="Username"
    />
    <ESegmented
      v-else-if="who === 'ai'"
      v-model="level"
      label="Level"
      size="sm"
      :options="levels"
      hint="1 is the weakest, 8 the strongest."
    />
    <p v-else class="note">Your bot answers by its own settings, so it has to be online.</p>
    <ESegmented
      v-model="clock"
      label="Clock"
      size="sm"
      :options="clocks"
      :hint="`${speed}, minutes plus seconds a move.`"
    />
    <ESegmented v-model="color" label="You play" size="sm" :options="colours" />
    <ESwitch
      v-model="rated"
      label="Rated"
      :disabled="casualOnly"
      :description="casualOnly ? 'Games against Stockfish are casual' : undefined"
    />
    <ENotice v-if="error" tone="danger" title="The challenge was not sent" :role="false">{{ error }}</ENotice>
    <EButton type="submit" :disabled="!ready" :loading="busy">Challenge</EButton>
  </form>
</template>

<style scoped>
.form {
  display: grid;
  justify-items: start;
  gap: var(--e-gap-base);
}

.form > :deep(.e-field),
.form > :deep(fieldset),
.form > :deep(.e-segmented) {
  max-inline-size: 100%;
}

.form > :deep(.e-field) {
  inline-size: 100%;
}

.note {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
