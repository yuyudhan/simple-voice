// FilePath: src/features/snippets/SnippetEditor.tsx
import { useState, type SyntheticEvent } from "react";
import { api, errorMessage, type Snippet } from "../../lib/api";
import { Button, Modal, TextArea, TextField } from "../../ui";

/** Mirrors the backend limits so the form says so before saving. */
const MAX_TRIGGER_CHARS = 60;
const MAX_TRIGGER_WORDS = 6;
const MAX_TEXT_CHARS = 4000;

export interface SnippetEditorProps {
    /** `null` adds a new snippet. */
    snippet: Snippet | null;
    onClose: () => void;
    onSaved: (snippet: Snippet, created: boolean) => void;
}

interface FieldErrors {
    trigger?: string;
    text?: string;
}

/** Add/edit form. Mount it with a `key` so each open starts from the snippet's values. */
export function SnippetEditor({ snippet, onClose, onSaved }: SnippetEditorProps) {
    const [trigger, setTrigger] = useState(snippet?.trigger ?? "");
    const [text, setText] = useState(snippet?.text ?? "");
    const [errors, setErrors] = useState<FieldErrors>({});
    const [saving, setSaving] = useState(false);

    async function save(event?: SyntheticEvent) {
        event?.preventDefault();
        const spoken = trigger.trim();
        const words = spoken.split(/\s+/).filter(Boolean).length;
        if (spoken.length === 0) {
            setErrors({ trigger: "Enter what you will say." });
            return;
        }
        if (spoken.length > MAX_TRIGGER_CHARS || words > MAX_TRIGGER_WORDS) {
            setErrors({
                trigger: `Keep it to ${String(MAX_TRIGGER_WORDS)} words and ${String(MAX_TRIGGER_CHARS)} characters.`,
            });
            return;
        }
        // The text is saved exactly as typed, line breaks and spacing included.
        if (text.trim().length === 0) {
            setErrors({ text: "Enter the text it expands to." });
            return;
        }
        if (text.length > MAX_TEXT_CHARS) {
            setErrors({
                text: `Keep it to ${String(MAX_TEXT_CHARS)} characters or fewer; this is ${String(text.length)}.`,
            });
            return;
        }
        setSaving(true);
        setErrors({});
        try {
            const saved = snippet
                ? await api.updateSnippet(snippet.id, spoken, text)
                : await api.addSnippet(spoken, text);
            onSaved(saved, snippet === null);
        } catch (error) {
            const message = errorMessage(error);
            // The backend names the offending field in its message; attach it there.
            setErrors(/trigger/i.test(message) ? { trigger: message } : { text: message });
        } finally {
            setSaving(false);
        }
    }

    return (
        <Modal
            open
            onClose={onClose}
            title={snippet ? "Edit snippet" : "New snippet"}
            width={520}
            footer={
                <>
                    <Button variant="ghost" onClick={onClose} disabled={saving}>
                        Cancel
                    </Button>
                    <Button
                        variant="primary"
                        loading={saving}
                        onClick={() => {
                            void save();
                        }}
                    >
                        {snippet ? "Save" : "Add"}
                    </Button>
                </>
            }
        >
            <form
                className="snippet-form"
                onSubmit={(event) => {
                    void save(event);
                }}
            >
                <TextField
                    label="When I say"
                    placeholder="e.g. my address"
                    value={trigger}
                    autoFocus
                    error={errors.trigger}
                    hint="Matched as whole words, ignoring case. A longer trigger wins over a shorter one inside it."
                    onChange={(event) => {
                        setTrigger(event.target.value);
                    }}
                />
                <TextArea
                    label="Insert"
                    placeholder={"e.g. 221B Baker Street\nLondon NW1 6XE"}
                    value={text}
                    rows={6}
                    error={errors.text}
                    hint="Inserted exactly as written, line breaks included. Formatting never rewrites it, and snippets inside it are not expanded again."
                    onChange={(event) => {
                        setText(event.target.value);
                    }}
                />
            </form>
        </Modal>
    );
}
