import { ref, watch, type Ref } from "vue";

/** A preference remembered in this browser, such as the board's orientation. Storage may be missing or blocked. */
export function stored<T>(key: string, initial: T): Ref<T> {
  let value = initial;
  try {
    const saved = localStorage.getItem(`chess.${key}`);
    if (saved !== null) value = JSON.parse(saved) as T;
  } catch {}
  const r = ref(value) as Ref<T>;
  watch(r, (v) => {
    try {
      localStorage.setItem(`chess.${key}`, JSON.stringify(v));
    } catch {}
  });
  return r;
}
