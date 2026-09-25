// FilePath: src/ui/Segmented.tsx
import { useRef, type KeyboardEvent, type ReactNode } from "react";
import "./Segmented.css";

export interface SegmentedOption<T extends string> {
    value: T;
    label: string;
    icon?: ReactNode;
}

export interface SegmentedProps<T extends string> {
    value: T;
    onChange: (value: T) => void;
    options: SegmentedOption<T>[];
    /** Accessible name of the radio group. */
    label: string;
}

const STEP: Partial<Record<string, number>> = {
    ArrowLeft: -1,
    ArrowUp: -1,
    ArrowRight: 1,
    ArrowDown: 1,
};

/** A radio group drawn as a segmented control; arrow keys move the selection (roving focus). */
export function Segmented<T extends string>({
    value,
    onChange,
    options,
    label,
}: SegmentedProps<T>) {
    const groupRef = useRef<HTMLDivElement>(null);

    function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
        const step = STEP[event.key];
        if (step === undefined) return;
        event.preventDefault();
        const current = Math.max(
            0,
            options.findIndex((option) => option.value === value),
        );
        const next = (current + step + options.length) % options.length;
        const target = options.at(next);
        if (!target) return;
        onChange(target.value);
        groupRef.current?.querySelectorAll<HTMLButtonElement>("[role=radio]")[next]?.focus();
    }

    return (
        <div
            ref={groupRef}
            className="sv-segmented"
            role="radiogroup"
            aria-label={label}
            onKeyDown={onKeyDown}
        >
            {options.map((option) => {
                const selected = option.value === value;
                return (
                    <button
                        key={option.value}
                        type="button"
                        role="radio"
                        aria-checked={selected}
                        tabIndex={selected ? 0 : -1}
                        className={`sv-segmented__item${selected ? " is-selected" : ""}`}
                        onClick={() => {
                            if (!selected) onChange(option.value);
                        }}
                    >
                        {option.icon && (
                            <span className="sv-segmented__icon" aria-hidden="true">
                                {option.icon}
                            </span>
                        )}
                        {option.label}
                    </button>
                );
            })}
        </div>
    );
}
