// FilePath: src/ui/TextArea.tsx
import { useId, type Ref, type TextareaHTMLAttributes } from "react";
import "./Field.css";

export type TextAreaProps = {
    label?: string;
    hint?: string;
    error?: string;
    ref?: Ref<HTMLTextAreaElement>;
} & TextareaHTMLAttributes<HTMLTextAreaElement>;

/** Multi-line counterpart of `TextField`: same label, hint and error wiring. */
export function TextArea({ label, hint, error, id, className, ref, ...rest }: TextAreaProps) {
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
            <textarea
                {...rest}
                ref={ref}
                id={inputId}
                className={`sv-input sv-textarea${error ? " has-error" : ""}`}
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
