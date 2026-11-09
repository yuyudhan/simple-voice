// FilePath: src/features/health/useHealth.ts
// The app's health as the backend reports it: loaded once, then replaced by every
// `health-changed` event, so a banner clears by itself when its cause goes away.
import { useEffect, useState } from "react";
import { api, events, type Health } from "../../lib/api";
import { useTauriEvent } from "../../lib/useTauriEvent";

export function useHealth(): Health | null {
    const [health, setHealth] = useState<Health | null>(null);

    useEffect(() => {
        let current = true;
        api.getHealth()
            .then((loaded) => {
                // An event may have arrived first; it is newer than this answer.
                if (current) setHealth((seen) => seen ?? loaded);
            })
            .catch((error: unknown) => {
                console.error("Could not load the app's health", error);
            });
        return () => {
            current = false;
        };
    }, []);

    useTauriEvent(events.healthChanged, setHealth);
    return health;
}
