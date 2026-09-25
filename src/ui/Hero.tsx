// FilePath: src/ui/Hero.tsx
import type { ReactNode } from "react";
import "./Hero.css";

export interface HeroProps {
    /** Wrap the accent word in `<em>` to render it in italic. */
    title: ReactNode;
    subtitle?: ReactNode;
    children?: ReactNode;
}

export function Hero({ title, subtitle, children }: HeroProps) {
    return (
        <section className="sv-hero">
            <div className="sv-hero__glow" aria-hidden="true" />
            <div className="sv-hero__content">
                <h2 className="sv-hero__title">{title}</h2>
                {subtitle && <p className="sv-hero__subtitle">{subtitle}</p>}
                {children && <div className="sv-hero__actions">{children}</div>}
            </div>
        </section>
    );
}
