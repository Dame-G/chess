import { onScopeDispose, ref, type Ref } from "vue";

/** Whether a media query matches now, kept up to date until the calling scope ends. */
export function useMedia(query: string): Ref<boolean> {
  const list = typeof matchMedia === "function" ? matchMedia(query) : undefined;
  const matches = ref(list?.matches ?? false);
  if (list) {
    const on = (e: MediaQueryListEvent) => (matches.value = e.matches);
    list.addEventListener("change", on);
    onScopeDispose(() => list.removeEventListener("change", on));
  }
  return matches;
}
