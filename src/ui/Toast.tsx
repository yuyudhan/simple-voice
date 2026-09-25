// FilePath: src/ui/Toast.tsx
import {
    createContext,
    useCallback,
    useContext,
    useEffect,
    useMemo,
    useRef,
    useState,
    type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { CircleAlert, CircleCheck, Info, X } from "lucide-react";
import "./Toast.css";

export type ToastTone = "neutral" | "success" | "danger";

interface ToastItem {
    id: number;
    message: string;
    tone: ToastTone;
}

interface ToastApi {
    toast: (message: string, tone?: ToastTone) => void;
}

const ToastContext = createContext<ToastApi | null>(null);

const MAX_VISIBLE = 4;

export function ToastProvider({ children }: { children: ReactNode }) {
    const [items, setItems] = useState<ToastItem[]>([]);
    const nextId = useRef(1);

    const dismiss = useCallback((id: number) => {
        setItems((current) => current.filter((item) => item.id !== id));
    }, []);

    const toast = useCallback((message: string, tone: ToastTone = "neutral") => {
        const id = nextId.current;
        nextId.current += 1;
        setItems((current) => [...current, { id, message, tone }].slice(-MAX_VISIBLE));
    }, []);

    const value = useMemo(() => ({ toast }), [toast]);

    return (
        <ToastContext.Provider value={value}>
            {children}
            {createPortal(
                <div className="sv-toasts" aria-live="polite" aria-relevant="additions">
                    {items.map((item) => (
                        <ToastView key={item.id} item={item} onDismiss={dismiss} />
                    ))}
                </div>,
                document.body,
            )}
        </ToastContext.Provider>
    );
}

function ToastView({ item, onDismiss }: { item: ToastItem; onDismiss: (id: number) => void }) {
    useEffect(() => {
        // Errors carry instructions (which permission to grant), so they stay up longer.
        const duration = item.tone === "danger" ? 7000 : 3200;
        const timer = window.setTimeout(() => {
            onDismiss(item.id);
        }, duration);
        return () => {
            window.clearTimeout(timer);
        };
    }, [item, onDismiss]);

    const icon =
        item.tone === "success" ? (
            <CircleCheck />
        ) : item.tone === "danger" ? (
            <CircleAlert />
        ) : (
            <Info />
        );

    return (
        <div
            className={`sv-toast sv-toast--${item.tone}`}
            role={item.tone === "danger" ? "alert" : "status"}
        >
            <span className="sv-toast__icon" aria-hidden="true">
                {icon}
            </span>
            <span className="sv-toast__message">{item.message}</span>
            <button
                type="button"
                className="sv-toast__close"
                aria-label="Dismiss"
                onClick={() => {
                    onDismiss(item.id);
                }}
            >
                <X aria-hidden="true" />
            </button>
        </div>
    );
}

export function useToast(): ToastApi {
    const context = useContext(ToastContext);
    if (!context) throw new Error("useToast must be used inside <ToastProvider>");
    return context;
}
