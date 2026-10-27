import { useRouter } from "vue-router";
import { useToast } from "@eos-ui/vue";
import { useDrills } from "../stores/drills";
import { usePolicy } from "../stores/policy";

/** What to tell someone who asked for drills and got none: new ones need reviewed games, old ones come back in time. */
function noDrillsText(total: number): string {
  return total === 0
    ? "Drills are made from the mistakes in your reviewed games. Review some games, then make drills from them."
    : "Everything you have is scheduled for later. They come back on the day they are due.";
}

/** Starts today's drills on the board (the `D` key, the command bar's "drills" and the You sheet's Start). */
export function useStartDrills() {
  const drills = useDrills();
  const policy = usePolicy();
  const router = useRouter();
  const toast = useToast();

  return async function startDrills() {
    if (!drills.available) {
      toast.show({ title: "Drills are off", message: policy.reason ?? undefined, tone: "info" });
      return;
    }
    try {
      if (!(await drills.start()) || !drills.active) {
        toast.show({ title: "No drills today", message: noDrillsText(drills.total), tone: "info" });
        return;
      }
      await router.push({ path: "/", query: { drill: String(drills.active.id) } });
    } catch (e) {
      toast.show({
        title: "The drills did not start",
        message: e instanceof Error ? e.message : String(e),
        tone: "warning",
      });
    }
  };
}
