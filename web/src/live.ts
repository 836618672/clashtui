import { ref, onActivated, onDeactivated, onUnmounted } from "vue";
import { useWorkspace, SessionChanged } from "./store";
export function useLive(load: () => Promise<void>, interval = 2000) {
  const s = useWorkspace(),
    error = ref("");
  let timer: ReturnType<typeof setTimeout>,
    generation = 0,
    active = false,
    running = false;
  async function tick(captured: number) {
    if (!active || captured !== generation) return;
    if (!running && s.connected && !s.busy && !document.hidden) {
      running = true;
      try {
        await load();
        if (captured === generation) error.value = "";
      } catch (e) {
        if (captured === generation && !(e instanceof SessionChanged)) {
          const previous = error.value;
          error.value = (e as Error).message;
          if (s.connected && !s.busy && previous !== error.value)
            s.notice(error.value, "error");
        }
      } finally {
        running = false;
      }
    }
    if (active && captured === generation)
      timer = setTimeout(() => tick(captured), interval);
  }
  onActivated(() => {
    active = true;
    generation++;
    void tick(generation);
  });
  const stop = () => {
    active = false;
    generation++;
    clearTimeout(timer);
  };
  onDeactivated(stop);
  onUnmounted(stop);
  return { error, refresh: load };
}
