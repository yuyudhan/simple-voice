// FilePath: src/ui/Badge.tsx
import type { ReactNode } from "react";
import "./Badge.css";

export type BadgeTone = "neutral" | "accent" | "success" | "warning" | "danger";

export function Badge({ tone = "neutral", children }: { tone?: BadgeTone; children: ReactNode }) {
    return <span className={`sv-badge sv-badge--${tone}`}>{children}</span>;
}
