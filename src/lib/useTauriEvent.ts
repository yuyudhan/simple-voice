// FilePath: src/lib/useTauriEvent.ts
import { useEffect, useRef } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";

/** Matches every `events.*` listener, including the payload-less `historyChanged`. */
type Subscribe<A extends unknown[]> = (callback: (...args: A) => void) => Promise<UnlistenFn>;

/**
 * Subscribes to one of the `events.*` listeners from `api.ts` for the lifetime of the component.
 * The handler may change every render; the subscription itself is made once per `subscribe`.
 */
export function useTauriEvent<A extends unknown[]>(
    subscribe: Subscribe<A>,
    handler: (...args: A) => void,
): void {
    const handlerRef = useRef(handler);

    useEffect(() => {
        handlerRef.current = handler;
    }, [handler]);

    useEffect(() => {
        let disposed = false;
        let unlisten: UnlistenFn | null = null;
        subscribe((...args: A) => {
            handlerRef.current(...args);
        })
            .then((fn) => {
                // Listening resolves asynchronously; the component may be gone by then.
                if (disposed) fn();
                else unlisten = fn;
            })
            .catch((error: unknown) => {
                console.error("Failed to subscribe to a Tauri event", error);
            });
        return () => {
            disposed = true;
            unlisten?.();
        };
    }, [subscribe]);
}
