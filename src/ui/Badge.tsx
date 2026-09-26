// FilePath: src/ui/Badge.tsx
import type { ReactNode } from "react";
import "./Badge.css";

export type BadgeTone = "neutral" | "accent" | "success" | "warning" | "danger";

export function Badge({
    tone = "neutral",
    title,
    children,
}: {
    tone?: BadgeTone;
    title?: string;
    children: ReactNode;
}) {
    return (
        <span className={`sv-badge sv-badge--${tone}`} title={title}>
            {children}
        </span>
    );
}
