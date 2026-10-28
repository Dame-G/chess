<script setup lang="ts">
import { computed, ref } from "vue";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import type { Idea } from "../api/types";
import { EConfirmDialog } from "@eos-ui/vue/components/EConfirmDialog";
import { useDameless } from "../stores/dameless";
import { useIdeas } from "../stores/ideas";
import { useVersions } from "../stores/versions";

/**
 * What can be done with an idea: stop it, resume it, and when it is kept, make it the new champion. Promoting changes
 * the fork on this machine only; pushing it to the remote is a separate, explicit step shown after, with its own
 * confirmation.
 */
const props = defineProps<{ idea: Idea }>();

const ideas = useIdeas();
const versions = useVersions();
const dameless = useDameless();
const toast = useToast();

const busy = ref<"stop" | "resume" | "promote" | "push" | null>(null);
const promoting = ref(false);
const pushing = ref(false);
const problem = ref<string | null>(null);
const pushed = ref<string | null>(null);

const version = computed(() => (props.idea.version != null ? versions.versions[props.idea.version] : undefined));
const promoted = computed(() => props.idea.status === "kept" && version.value?.line.type === "champion");
const canPromote = computed(() => props.idea.status === "kept" && !promoted.value);
const next = computed(() => `v${(versions.current?.line.type === "champion" ? versions.current.line.number : 0) + 1}`);
const pushLabel = computed(() => (pushed.value ? "Push again" : "Push the fork"));
const change = computed(() => version.value?.name ?? props.idea.branch);

async function run(what: NonNullable<typeof busy.value>, action: () => Promise<unknown>) {
  busy.value = what;
  problem.value = null;
  try {
    await action();
    return true;
  } catch (e) {
    problem.value = e instanceof Error ? e.message : String(e);
    return false;
  } finally {
    busy.value = null;
  }
}

async function promote() {
  if (await run("promote", () => ideas.promote(props.idea.id))) {
    promoting.value = false;
    toast.show({
      title: `${change.value} is the new champion`,
      message: "Made on this machine. Nothing is pushed.",
      tone: "success",
    });
  }
}

async function push() {
  if (await run("push", async () => (pushed.value = (await dameless.push()).head))) {
    pushing.value = false;
    toast.show({ title: "The fork is pushed", message: "main is now on origin.", tone: "success" });
  }
}
</script>

<template>
  <div class="actions">
    <div class="sheet-actions">
      <EButton v-if="canPromote" :loading="busy === 'promote'" @click="promoting = true">Promote</EButton>
      <EButton
        v-if="idea.status === 'running' || idea.status === 'queued'"
        variant="outline"
        :loading="busy === 'stop'"
        @click="run('stop', () => ideas.stop(idea.id))"
      >
        Stop
      </EButton>
      <EButton
        v-if="idea.status === 'stopped'"
        :loading="busy === 'resume'"
        @click="run('resume', () => ideas.resume(idea.id))"
      >
        Resume
      </EButton>
      <EButton v-if="promoted" variant="outline" @click="pushing = true">
        {{ pushLabel }}
      </EButton>
    </div>
    <p v-if="promoted && !pushed" class="sec-note">
      Promoted on this machine. The fork's main has not been pushed to the remote.
    </p>
    <p v-if="pushed" class="sec-note">Pushed at {{ pushed }}.</p>
    <ENotice v-if="problem && !promoting && !pushing" tone="danger" title="That did not work">{{ problem }}</ENotice>

    <EConfirmDialog
      v-model:open="promoting"
      :title="`Make ${change} the new champion ${next}`"
      description="Its change is merged into the fork's main on this machine."
      :confirm-label="`Promote to ${next}`"
      :loading="busy === 'promote'"
      :close-on-confirm="false"
      @confirm="promote"
    >
      <p class="text">
        Future ideas will be tested against {{ next }}. If the checkout has uncommitted changes, nothing is merged and
        you are told why. Nothing is pushed.
      </p>
      <ENotice v-if="problem" tone="danger" title="It was not promoted">{{ problem }}</ENotice>
    </EConfirmDialog>

    <EConfirmDialog
      v-model:open="pushing"
      title="Push the fork to its remote?"
      description="This publishes the fork's main, with every promotion in it."
      confirm-label="Push main"
      :loading="busy === 'push'"
      :close-on-confirm="false"
      @confirm="push"
    >
      <p class="text">Pushing is never automatic. Once pushed, other people can fetch these commits.</p>
      <ENotice v-if="problem" tone="danger" title="It was not pushed">{{ problem }}</ENotice>
    </EConfirmDialog>
  </div>
</template>

<style scoped>
.actions {
  display: grid;
  gap: var(--e-gap-snug);
}

.text {
  margin: 0;
}
</style>
