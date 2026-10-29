import { computed, ref, type MaybeRefOrGetter, toValue } from "vue";
import { useToast } from "@eos-ui/vue";
import { progressText } from "../format";
import { usePlayers } from "../stores/players";
import { useWork } from "../stores/work";

/**
 * Bringing a player's Lichess games in: starts the import, and says whether one is already queued or running for them
 * (the first brings in the whole history, later ones only what is new), with its progress as one line.
 */
export function useImport(key: MaybeRefOrGetter<string>) {
  const players = usePlayers();
  const work = useWork();
  const toast = useToast();
  const busy = ref(false);

  const underway = computed(() =>
    [...work.running, ...work.queued].find(
      (w) => w.kind === "import" && w.subject.type === "player" && w.subject.key === toValue(key),
    ),
  );
  const line = computed(() => {
    const w = underway.value;
    return w ? `${w.title}: ${progressText(w.progress) ?? "starting"}.` : "";
  });

  async function run(max?: number) {
    busy.value = true;
    try {
      await players.importHistory(toValue(key), max ? { max } : {});
    } catch (e) {
      toast.show({ title: "That did not start", message: e instanceof Error ? e.message : String(e), tone: "warning" });
    } finally {
      busy.value = false;
    }
  }

  return { busy, underway, line, run };
}
