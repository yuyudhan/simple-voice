// FilePath: src/ui/Select.tsx
import { useId } from "react";
import { ChevronDown } from "lucide-react";
import "./Field.css";

export interface SelectOption<T extends string> {
    value: T;
    label: string;
}

export interface SelectProps<T extends string> {
    value: T;
    onChange: (value: T) => void;
    options: SelectOption<T>[];
    label?: string;
}

export function Select<T extends string>({ value, onChange, options, label }: SelectProps<T>) {
    const id = useId();
    return (
        <div className="sv-field sv-field--inline">
            {label && (
                <label className="sv-field__label" htmlFor={id}>
                    {label}
                </label>
            )}
            <div className="sv-select">
                <select
                    id={id}
                    value={value}
                    onChange={(event) => {
                        // Map back through the typed options instead of trusting the DOM string.
                        const picked = options.find(
                            (option) => option.value === event.target.value,
                        );
                        if (picked) onChange(picked.value);
                    }}
                >
                    {options.map((option) => (
                        <option key={option.value} value={option.value}>
                            {option.label}
                        </option>
                    ))}
                </select>
                <ChevronDown className="sv-select__chevron" aria-hidden="true" />
            </div>
        </div>
    );
}
