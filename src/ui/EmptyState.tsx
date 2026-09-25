// FilePath: src/ui/EmptyState.tsx
import type { ReactNode } from "react";
import "./EmptyState.css";

export interface EmptyStateProps {
    icon: ReactNode;
    title: string;
    description?: string;
    action?: ReactNode;
}

export function EmptyState({ icon, title, description, action }: EmptyStateProps) {
    return (
        <div className="sv-empty">
            <div className="sv-empty__icon" aria-hidden="true">
                {icon}
            </div>
            <h3 className="sv-empty__title">{title}</h3>
            {description && <p className="sv-empty__description">{description}</p>}
            {action && <div className="sv-empty__action">{action}</div>}
        </div>
    );
}
