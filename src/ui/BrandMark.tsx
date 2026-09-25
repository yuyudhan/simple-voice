// FilePath: src/ui/BrandMark.tsx
import "./BrandMark.css";

/** The product's meter-bar mark: four rounded ink bars, the loudest one lit in signal orange. */
export function BrandMark({ className }: { className?: string }) {
    return (
        <svg
            className={className ? `sv-brand-mark ${className}` : "sv-brand-mark"}
            viewBox="0 0 20 20"
            aria-hidden="true"
        >
            <rect x="2" y="7.5" width="2.4" height="5" rx="1.2" />
            <rect x="6.2" y="4.5" width="2.4" height="11" rx="1.2" />
            <rect
                className="sv-brand-mark__signal"
                x="10.4"
                y="2"
                width="2.4"
                height="16"
                rx="1.2"
            />
            <rect x="14.6" y="6" width="2.4" height="8" rx="1.2" />
        </svg>
    );
}
