import { computed, reactive, ref } from "vue";
import { useToast } from "@eos-ui/vue";
import type { Idea } from "../api/types";
import { useDameless } from "../stores/dameless";
import { useIdeas } from "../stores/ideas";
import { useVersions } from "../stores/versions";
import { useConfirm } from "../ui/useConfirm";

type Busy = "stop" | "resume" | "promote" | "push";

/**
 * What can be done with an idea: stop it, resume it, and when it is kept, make it the new champion. Promoting changes
 * the fork on this machine only; pushing it to the remote is a separate, explicit step shown after, with its own
 * confirmation. The state lives here so the sheet can put the buttons in its header and what they did in its body.
 */
export function useIdeaActions(idea: () => Idea) {
  const ideas = useIdeas();
  const versions = useVersions();
  const dameless = useDameless();
  const toast = useToast();
  const confirm = useConfirm();

  const busy = ref<Busy | null>(null);
  /** Why the last action did not work, said in the sheet until the next try. */
  const problem = ref<string | null>(null);
  const pushed = ref<string | null>(null);

  const version = computed(() => {
    const id = idea().version;
    return id != null ? versions.versions[id] : undefined;
  });
  const promoted = computed(() => idea().status === "kept" && version.value?.line.type === "champion");
  const canPromote = computed(() => idea().status === "kept" && !promoted.value);
  const next = computed(
    () => `v${(versions.current?.line.type === "champion" ? versions.current.line.number : 0) + 1}`,
  );
  const change = computed(() => version.value?.name ?? idea().branch);

  async function run(what: Busy, action: () => Promise<unknown>): Promise<boolean> {
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

  const stop = () => run("stop", () => ideas.stop(idea().id));
  const resume = () => run("resume", () => ideas.resume(idea().id));

  async function promote() {
    const yes = await confirm({
      title: `Promote to ${next.value}`,
      body: `${change.value} is merged into the fork's main on this machine and becomes the champion. Future ideas are tried against ${next.value}. Nothing is pushed.`,
      confirmLabel: `Promote to ${next.value}`,
    });
    if (!yes) return;
    if (await run("promote", () => ideas.promote(idea().id))) {
      toast.show({
        title: `${change.value} is the new champion`,
        message: "Made on this machine. Nothing is pushed.",
        tone: "success",
      });
    }
  }

  async function pushFork() {
    const yes = await confirm({
      title: "Push the fork?",
      body: "This publishes the fork's main, with every promotion in it. Once pushed, other people can fetch these commits.",
      confirmLabel: "Push main",
    });
    if (!yes) return;
    if (await run("push", async () => (pushed.value = (await dameless.push()).head))) {
      toast.show({ title: "The fork is pushed", message: "main is now on origin.", tone: "success" });
    }
  }

  return reactive({ busy, problem, pushed, promoted, canPromote, next, promote, pushFork, stop, resume });
}

export type IdeaActionsState = ReturnType<typeof useIdeaActions>;
