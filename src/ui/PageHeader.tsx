// FilePath: src/ui/PageHeader.tsx
import type { ReactNode } from "react";
import "./PageHeader.css";

export interface PageHeaderProps {
    title: ReactNode;
    description?: ReactNode;
    /** Page-level actions, aligned to the right of the title. */
    actions?: ReactNode;
}

/** The opening of every page: title, one-line description, actions, then the level line. */
export function PageHeader({ title, description, actions }: PageHeaderProps) {
    return (
        <header className="sv-page-header">
            <div className="sv-page-header__row">
                <div className="sv-page-header__text">
                    <h1 className="page-title">{title}</h1>
                    {description && <p className="page-subtitle">{description}</p>}
                </div>
                {actions && <div className="sv-page-header__actions">{actions}</div>}
            </div>
            <div className="sv-level-line" aria-hidden="true" />
        </header>
    );
}
