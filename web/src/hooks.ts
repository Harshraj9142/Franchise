import { useCallback, useEffect, useRef, useState } from "react";

/** Polls an async loader so job results and sales show up live. */
export function usePoll<T>(load: () => Promise<T>, deps: unknown[], ms = 4000) {
  const [data, setData] = useState<T | undefined>();
  const [error, setError] = useState<string | null>(null);
  const loader = useRef(load);
  loader.current = load;
  const refresh = useCallback(async () => {
    try {
      setData(await loader.current());
      setError(null);
    } catch (e) {
      setError((e as Error).message);
    }
  }, []);
  useEffect(() => {
    setData(undefined);
    refresh();
    // Don't poll the RPC from background tabs; catch up as soon as the tab is visible again.
    const t = setInterval(() => document.visibilityState === "visible" && refresh(), ms);
    const onVisible = () => document.visibilityState === "visible" && refresh();
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      clearInterval(t);
      document.removeEventListener("visibilitychange", onVisible);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  return { data, error, refresh };
}
