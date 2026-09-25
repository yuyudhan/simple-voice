// FilePath: src/ui/Modal.tsx
import { useEffect, useId, useRef, type KeyboardEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";
import { IconButton } from "./Button";
import "./Modal.css";

export interface ModalProps {
    open: boolean;
    onClose: () => void;
    title?: string;
    width?: number;
    children: ReactNode;
    footer?: ReactNode;
}

// Everything reachable with Tab; tabindex="-1" elements are focusable but deliberately skipped.
const FOCUSABLE = [
    "a[href]",
    "button:not([disabled])",
    "input:not([disabled])",
    "select:not([disabled])",
    "textarea:not([disabled])",
    "[tabindex]",
]
    .map((selector) => `${selector}:not([tabindex="-1"])`)
    .join(", ");

// The last two focused elements, so focus can return to the opener even when a field inside
// the dialog grabbed focus (autoFocus) before the dialog's effect ran.
const focusHistory: { current: HTMLElement | null; previous: HTMLElement | null } = {
    current: null,
    previous: null,
};
document.addEventListener("focusin", (event) => {
    if (!(event.target instanceof HTMLElement) || event.target === focusHistory.current) return;
    focusHistory.previous = focusHistory.current;
    focusHistory.current = event.target;
});

// Modals can stack (a confirm inside Settings); only the topmost one reacts to Esc.
const openStack: symbol[] = [];

export function Modal({ open, onClose, title, width = 480, children, footer }: ModalProps) {
    const dialogRef = useRef<HTMLDivElement>(null);
    const onCloseRef = useRef(onClose);
    const titleId = useId();

    useEffect(() => {
        onCloseRef.current = onClose;
    }, [onClose]);

    useEffect(() => {
        if (!open) return undefined;
        const token = Symbol("modal");
        openStack.push(token);
        const dialog = dialogRef.current;
        // React applies `autoFocus` before effects run, so focus may already be inside.
        const active = document.activeElement;
        const focusedInside = dialog?.contains(active) ?? false;
        const opener = active instanceof HTMLElement && active !== document.body ? active : null;
        const previouslyFocused = focusedInside ? focusHistory.previous : opener;
        if (dialog && !focusedInside) {
            (dialog.querySelector<HTMLElement>(FOCUSABLE) ?? dialog).focus();
        }
        function onKey(event: globalThis.KeyboardEvent) {
            if (event.key !== "Escape" || openStack[openStack.length - 1] !== token) return;
            event.preventDefault();
            event.stopPropagation();
            onCloseRef.current();
        }
        window.addEventListener("keydown", onKey, true);
        return () => {
            window.removeEventListener("keydown", onKey, true);
            const index = openStack.indexOf(token);
            if (index >= 0) openStack.splice(index, 1);
            previouslyFocused?.focus();
        };
    }, [open]);

    function trapFocus(event: KeyboardEvent<HTMLDivElement>) {
        if (event.key !== "Tab" || !dialogRef.current) return;
        const nodes = Array.from(dialogRef.current.querySelectorAll<HTMLElement>(FOCUSABLE));
        const first = nodes[0];
        const last = nodes[nodes.length - 1];
        if (!first || !last) {
            event.preventDefault();
            return;
        }
        if (event.shiftKey && document.activeElement === first) {
            event.preventDefault();
            last.focus();
        } else if (!event.shiftKey && document.activeElement === last) {
            event.preventDefault();
            first.focus();
        }
    }

    if (!open) return null;

    return createPortal(
        <div
            className="sv-modal-backdrop"
            onMouseDown={(event) => {
                if (event.target === event.currentTarget) onClose();
            }}
        >
            <div
                ref={dialogRef}
                className="sv-modal"
                role="dialog"
                aria-modal="true"
                aria-labelledby={title ? titleId : undefined}
                tabIndex={-1}
                style={{ width: `min(${String(width)}px, calc(100vw - 48px))` }}
                onKeyDown={trapFocus}
            >
                {title && (
                    <header className="sv-modal__header">
                        <h2 id={titleId} className="sv-modal__title">
                            {title}
                        </h2>
                        <IconButton label="Close" icon={<X />} onClick={onClose} />
                    </header>
                )}
                <div className="sv-modal__body">{children}</div>
                {footer && <footer className="sv-modal__footer">{footer}</footer>}
            </div>
        </div>,
        document.body,
    );
}
