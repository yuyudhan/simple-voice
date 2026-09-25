// FilePath: src/ui/Card.tsx
import type { ReactNode } from "react";
import "./Card.css";

export interface CardProps {
    children: ReactNode;
    className?: string;
    padded?: boolean;
}

export function Card({ children, className, padded = true }: CardProps) {
    const classes = ["sv-card", padded && "sv-card--padded", className].filter(Boolean).join(" ");
    return <div className={classes}>{children}</div>;
}

export interface SettingsGroupProps {
    title?: string;
    children: ReactNode;
}

export function SettingsGroup({ title, children }: SettingsGroupProps) {
    return (
        <section className="sv-settings-group">
            {title && <h3 className="sv-settings-group__title caps-label">{title}</h3>}
            <div className="sv-settings-group__body">{children}</div>
        </section>
    );
}

export interface SettingRowProps {
    title: ReactNode;
    description?: ReactNode;
    children?: ReactNode;
}

export function SettingRow({ title, description, children }: SettingRowProps) {
    return (
        <div className="sv-setting-row">
            <div className="sv-setting-row__text">
                <div className="sv-setting-row__title">{title}</div>
                {description && <div className="sv-setting-row__description">{description}</div>}
            </div>
            {children && <div className="sv-setting-row__control">{children}</div>}
        </div>
    );
}
