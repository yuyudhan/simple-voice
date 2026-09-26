// FilePath: src/ui/TextField.tsx
import { useId, type InputHTMLAttributes, type Ref } from "react";
import "./Field.css";

export type TextFieldProps = {
    label?: string;
    hint?: string;
    error?: string;
    ref?: Ref<HTMLInputElement>;
} & InputHTMLAttributes<HTMLInputElement>;

export function TextField({ label, hint, error, id, className, ref, ...rest }: TextFieldProps) {
    const generatedId = useId();
    const inputId = id ?? generatedId;
    const messageId = `${inputId}-message`;
    const message = error ?? hint;
    return (
        <div className={["sv-field", className].filter(Boolean).join(" ")}>
            {label && (
                <label className="sv-field__label" htmlFor={inputId}>
                    {label}
                </label>
            )}
            <input
                {...rest}
                ref={ref}
                id={inputId}
                className={`sv-input${error ? " has-error" : ""}`}
                aria-invalid={error ? true : undefined}
                aria-describedby={message ? messageId : undefined}
            />
            {message && (
                <div
                    id={messageId}
                    className={`sv-field__message${error ? " is-error" : ""}`}
                    role={error ? "alert" : undefined}
                >
                    {message}
                </div>
            )}
        </div>
    );
}
