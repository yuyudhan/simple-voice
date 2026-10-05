// FilePath: src/app/PlatformContext.tsx
import { createContext, useContext } from "react";

/** Which app the shared pages render in. The desktop app needs no provider. */
export type Platform = "desktop" | "mobile";

export const PlatformContext = createContext<Platform>("desktop");

export function usePlatform(): Platform {
    return useContext(PlatformContext);
}

/** The device named in copy such as "every dictation on this Mac". */
export function useDeviceName(): string {
    return usePlatform() === "mobile" ? "phone" : "Mac";
}
