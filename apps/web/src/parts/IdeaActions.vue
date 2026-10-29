<script setup lang="ts">
import { EButton } from "@eos-ui/vue/components/EButton";
import type { Idea } from "../api/types";
import type { IdeaActionsState } from "./ideaActions";

/**
 * The buttons an idea allows, for the sheet's header: Stop while it runs, Resume when it was stopped, and the one
 * primary, Promote, only when it is kept. After promoting, Push the fork is a quiet button of its own. What they did
 * (or why they did not work) is said in the sheet's body from the same state.
 */
defineProps<{ idea: Idea; actions: IdeaActionsState }>();
</script>

<template>
  <EButton v-if="actions.canPromote" size="sm" :loading="actions.busy === 'promote'" @click="actions.promote()">
    Promote to {{ actions.next }}
  </EButton>
  <EButton
    v-if="idea.status === 'running' || idea.status === 'queued'"
    variant="quiet"
    size="sm"
    :loading="actions.busy === 'stop'"
    @click="actions.stop()"
  >
    Stop
  </EButton>
  <EButton
    v-if="idea.status === 'stopped'"
    variant="quiet"
    size="sm"
    :loading="actions.busy === 'resume'"
    @click="actions.resume()"
  >
    Resume
  </EButton>
  <EButton
    v-if="actions.promoted"
    variant="quiet"
    size="sm"
    :loading="actions.busy === 'push'"
    @click="actions.pushFork()"
  >
    {{ actions.pushed ? "Push again" : "Push the fork" }}
  </EButton>
</template>
