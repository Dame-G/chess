import { onBeforeUnmount, onMounted, reactive, ref } from "vue";
import { useRouter } from "vue-router";
import { useBoard } from "../board/board";

/**
 * What every sheet binds to its `ESheet`: open while its route is, 32rem wide beside the board, and the whole window
 * below 75rem. Closing (the button, Escape) returns to the board as it was; `back` goes to the previous sheet when
 * there is one.
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
    // A truncated title keeps its full text for a pointer to read.
    const title = document.querySelector<HTMLElement>(".e-sheet-title");
    if (title) {
      const sync = () => (title.title = title.textContent ?? "");
      sync();
      const watch = new MutationObserver(sync);
      watch.observe(title, { childList: true, characterData: true, subtree: true });
      onBeforeUnmount(() => watch.disconnect());
    }
    const back = (window.history.state as { back?: string | null } | null)?.back ?? null;
    canGoBack.value = !!back && !back.startsWith("/?") && back !== "/";
  });
  function back() {
    if (canGoBack.value) router.back();
    else close();
  }

  const bind = { open: true, size: "md" as const, modalBelow: "75rem", onClose: close };
  return reactive({ bind, canGoBack, back });
}
