// FilePath: src/ui/Toggle.tsx
import "./Toggle.css";

export interface ToggleProps {
    checked: boolean;
    onChange: (value: boolean) => void;
    disabled?: boolean;
    label: string;
}

export function Toggle({ checked, onChange, disabled = false, label }: ToggleProps) {
    return (
        <button
            type="button"
            className={checked ? "sv-toggle is-on" : "sv-toggle"}
            role="switch"
            aria-checked={checked}
            aria-label={label}
            disabled={disabled}
            onClick={() => {
                onChange(!checked);
            }}
        >
            <span className="sv-toggle__thumb" />
        </button>
    );
}
