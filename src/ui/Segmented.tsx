// FilePath: src/ui/Segmented.tsx
import type { ReactNode } from "react";
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
}

export function Segmented<T extends string>({ value, onChange, options }: SegmentedProps<T>) {
    return (
        <div className="sv-segmented" role="radiogroup">
            {options.map((option) => {
                const selected = option.value === value;
                return (
                    <button
                        key={option.value}
                        type="button"
                        role="radio"
                        aria-checked={selected}
                        className={`sv-segmented__item${selected ? " is-selected" : ""}`}
                        onClick={() => {
                            onChange(option.value);
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
