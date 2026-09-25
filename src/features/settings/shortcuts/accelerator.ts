// FilePath: src/features/settings/shortcuts/accelerator.ts
// Builds Tauri global-shortcut accelerator strings from DOM keyboard events. `event.code` is used
// instead of `event.key` because Option on macOS rewrites `key` (Option+A yields "å"), while the
// physical code stays stable and matches the names the global-shortcut parser understands.

export const MODIFIER_ORDER = ["Control", "Alt", "Shift", "Super"] as const;
export type Modifier = (typeof MODIFIER_ORDER)[number];

/** The Fn (Globe) key on its own; the core watches it outside the global-shortcut plugin. */
export const FN_KEY = "Fn";

const NAMED_CODES: Record<string, string> = {
    Space: "Space",
    Quote: "Quote",
    Semicolon: "Semicolon",
    Comma: "Comma",
    Period: "Period",
    Slash: "Slash",
    Backslash: "Backslash",
    BracketLeft: "BracketLeft",
    BracketRight: "BracketRight",
    Backquote: "Backquote",
    Minus: "Minus",
    Equal: "Equal",
    ArrowUp: "ArrowUp",
    ArrowDown: "ArrowDown",
    ArrowLeft: "ArrowLeft",
    ArrowRight: "ArrowRight",
};

const MODIFIER_CODES: Record<string, true> = {
    ControlLeft: true,
    ControlRight: true,
    AltLeft: true,
    AltRight: true,
    ShiftLeft: true,
    ShiftRight: true,
    MetaLeft: true,
    MetaRight: true,
    CapsLock: true,
    Fn: true,
    FnLock: true,
};

const FUNCTION_KEY = /^F([1-9]|1[0-2])$/;

export type CaptureResult =
    | { kind: "partial"; modifiers: Modifier[] }
    | { kind: "complete"; accelerator: string }
    | { kind: "invalid"; modifiers: Modifier[]; reason: string };

/** Interprets one keydown while recording a shortcut. */
export function captureKey(event: KeyboardEvent): CaptureResult {
    const held: Record<Modifier, boolean> = {
        Control: event.ctrlKey,
        Alt: event.altKey,
        Shift: event.shiftKey,
        Super: event.metaKey,
    };
    const modifiers = MODIFIER_ORDER.filter((m) => held[m]);
    if (MODIFIER_CODES[event.code]) return { kind: "partial", modifiers };

    let key: string | null = NAMED_CODES[event.code] ?? null;
    if (/^Key[A-Z]$/.test(event.code)) key = event.code.slice(3);
    else if (/^Digit[0-9]$/.test(event.code)) key = event.code.slice(5);
    else if (FUNCTION_KEY.test(event.code)) key = event.code;

    if (key === null) {
        return { kind: "invalid", modifiers, reason: "That key cannot be used in a shortcut." };
    }
    if (modifiers.length === 0 && !FUNCTION_KEY.test(key)) {
        return {
            kind: "invalid",
            modifiers,
            reason: "Add a modifier such as ⌃, ⌥, ⇧ or ⌘ (function keys work on their own).",
        };
    }
    return { kind: "complete", accelerator: [...modifiers, key].join("+") };
}
