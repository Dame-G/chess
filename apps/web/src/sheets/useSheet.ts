import { computed, onMounted, reactive, ref } from "vue";
import { useRouter } from "vue-router";
import { useBoard } from "../board/board";

/**
 * What every sheet binds to its `ESheet`: open while its route is, 32rem wide beside the board, and the whole window
 * below 75rem (sliding in, so the board is never seen through it), with its text kept to a readable line. Closing (the
 * button, Escape) returns to the board as it was; Back goes to the previous sheet when there is one.
 */
export function useSheet() {
  const router = useRouter();
  const board = useBoard();

  function close() {
    void router.push({ path: "/", query: board.address });
  }

  /** Whether the page before this one was another sheet, so Back can go there. */
  const canGoBack = ref(false);
  onMounted(() => {
    const back = (window.history.state as { back?: string | null } | null)?.back ?? null;
    canGoBack.value = !!back && !back.startsWith("/?") && back !== "/";
  });
  function back() {
    if (canGoBack.value) router.back();
    else close();
  }

  const bind = computed(() => ({
    open: true,
    size: "md" as const,
    modalBelow: "75rem",
    measure: true,
    transition: "slide" as const,
    back: canGoBack.value,
    onBack: back,
    onClose: close,
  }));
  return reactive({ bind });
}
