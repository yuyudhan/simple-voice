// FilePath: src/ui/Button.tsx
import type { ButtonHTMLAttributes, ReactNode } from "react";
import { Spinner } from "./Spinner";
import "./Button.css";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";

export type ButtonProps = {
    variant?: ButtonVariant;
    size?: "sm" | "md";
    icon?: ReactNode;
    loading?: boolean;
} & ButtonHTMLAttributes<HTMLButtonElement>;

export function Button({
    variant = "secondary",
    size = "md",
    icon,
    loading = false,
    className,
    children,
    disabled,
    type = "button",
    ...rest
}: ButtonProps) {
    const classes = ["sv-btn", `sv-btn--${variant}`, `sv-btn--${size}`, className]
        .filter(Boolean)
        .join(" ");
    return (
        <button
            {...rest}
            type={type}
            className={classes}
            disabled={disabled === true || loading}
            aria-busy={loading || undefined}
        >
            {loading ? (
                <Spinner size={size === "sm" ? 12 : 14} />
            ) : (
                icon && (
                    <span className="sv-btn__icon" aria-hidden="true">
                        {icon}
                    </span>
                )
            )}
            {children !== undefined && children !== null && (
                <span className="sv-btn__label">{children}</span>
            )}
        </button>
    );
}

export type IconButtonProps = {
    label: string;
    icon: ReactNode;
} & ButtonHTMLAttributes<HTMLButtonElement>;

export function IconButton({ label, icon, className, type = "button", ...rest }: IconButtonProps) {
    return (
        <button
            title={label}
            {...rest}
            type={type}
            aria-label={label}
            className={["sv-icon-btn", className].filter(Boolean).join(" ")}
        >
            <span aria-hidden="true">{icon}</span>
        </button>
    );
}
