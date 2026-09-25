// FilePath: src/ui/Kbd.tsx
import type { ReactNode } from "react";
import "./Kbd.css";

export function Kbd({ children }: { children: ReactNode }) {
    return <kbd className="sv-kbd">{children}</kbd>;
}

const MODIFIERS: Record<string, { symbol: string; name: string; order: number }> = {
    control: { symbol: "⌃", name: "Control", order: 0 },
    ctrl: { symbol: "⌃", name: "Control", order: 0 },
    alt: { symbol: "⌥", name: "Option", order: 1 },
    option: { symbol: "⌥", name: "Option", order: 1 },
    shift: { symbol: "⇧", name: "Shift", order: 2 },
    super: { symbol: "⌘", name: "Command", order: 3 },
    cmd: { symbol: "⌘", name: "Command", order: 3 },
    command: { symbol: "⌘", name: "Command", order: 3 },
    meta: { symbol: "⌘", name: "Command", order: 3 },
    cmdorctrl: { symbol: "⌘", name: "Command", order: 3 },
    commandorcontrol: { symbol: "⌘", name: "Command", order: 3 },
};

const KEYS: Record<string, string> = {
    fn: "fn",
    quote: "'",
    space: "Space",
    comma: ",",
    period: ".",
    slash: "/",
    backslash: "\\",
    semicolon: ";",
    minus: "-",
    equal: "=",
    plus: "+",
    backquote: "`",
    bracketleft: "[",
    bracketright: "]",
    enter: "↩",
    return: "↩",
    escape: "Esc",
    esc: "Esc",
    backspace: "⌫",
    delete: "⌦",
    tab: "⇥",
    arrowup: "↑",
    arrowdown: "↓",
    arrowleft: "←",
    arrowright: "→",
    up: "↑",
    down: "↓",
    left: "←",
    right: "→",
};

export interface KeyCap {
    label: string;
    name: string;
}

/** Turns a Tauri accelerator ("Control+Shift+Space") into macOS keycaps, modifiers first. */
export function acceleratorKeys(accelerator: string): KeyCap[] {
    const parts = accelerator.split("+").filter((part) => part.length > 0);
    if (accelerator.endsWith("++") || accelerator === "+") parts.push("+");
    const modifiers: { cap: KeyCap; order: number }[] = [];
    const keys: KeyCap[] = [];
    for (const raw of parts) {
        const part = raw.trim();
        const lower = part.toLowerCase();
        const modifier = MODIFIERS[lower];
        if (modifier) {
            modifiers.push({
                cap: { label: modifier.symbol, name: modifier.name },
                order: modifier.order,
            });
            continue;
        }
        const named = KEYS[lower];
        if (named !== undefined) {
            keys.push({ label: named, name: part });
        } else if (/^key[a-z]$/i.test(part)) {
            keys.push({ label: part.slice(3).toUpperCase(), name: part.slice(3).toUpperCase() });
        } else if (/^digit[0-9]$/i.test(part)) {
            keys.push({ label: part.slice(5), name: part.slice(5) });
        } else {
            const label = part.length === 1 ? part.toUpperCase() : part;
            keys.push({ label, name: label });
        }
    }
    modifiers.sort((a, b) => a.order - b.order);
    return [...modifiers.map((m) => m.cap), ...keys];
}

export function ShortcutKeys({ accelerator }: { accelerator: string }) {
    const caps = acceleratorKeys(accelerator);
    const spoken = caps.map((cap) => cap.name).join(" ");
    return (
        <span className="sv-shortcut" aria-label={spoken} role="img">
            {caps.map((cap, index) => (
                <Kbd key={`${cap.name}-${String(index)}`}>{cap.label}</Kbd>
            ))}
        </span>
    );
}
