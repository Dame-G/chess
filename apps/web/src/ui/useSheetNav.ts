import { inject, onMounted, ref } from "vue";
import { routerKey } from "vue-router";
import { useBoard } from "../board/board";

/**
 * How a sheet leaves: Close returns to the board as it was (its address in the query), and Back goes to the previous
 * sheet when the page before this one was one, else it closes. With no router (the kit page) both do nothing. The
 * board's store is only touched when a sheet is actually left.
 */
export function useSheetNav() {
  const router = inject(routerKey, null);

  function close() {
    void router?.push({ path: "/", query: useBoard().address });
  }

  /** Whether the page before this one was another sheet, so Back can go there. */
  const canGoBack = ref(false);
  onMounted(() => {
    const back = (window.history.state as { back?: string | null } | null)?.back ?? null;
    canGoBack.value = !!back && !back.startsWith("/?") && back !== "/";
  });

  function back() {
    if (canGoBack.value) router?.back();
    else close();
  }

  return { close, back, canGoBack };
}
