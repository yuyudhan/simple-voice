// FilePath: src/features/dictionary/DictionaryEditor.tsx
import { useState, type SyntheticEvent } from "react";
import { ArrowRight } from "lucide-react";
import { api, errorMessage, type DictionaryEntry } from "../../lib/api";
import { Button, Modal, TextField, Toggle } from "../../ui";

export interface DictionaryEditorProps {
    /** `null` adds a new entry. */
    entry: DictionaryEntry | null;
    onClose: () => void;
    onSaved: (entry: DictionaryEntry, created: boolean) => void;
}

interface FieldErrors {
    phrase?: string;
    replacement?: string;
}

/** Add/edit form. Mount it with a `key` so each open starts from the entry's values. */
export function DictionaryEditor({ entry, onClose, onSaved }: DictionaryEditorProps) {
    const [phrase, setPhrase] = useState(entry?.phrase ?? "");
    const [isRule, setIsRule] = useState((entry?.replacement ?? null) !== null);
    const [replacement, setReplacement] = useState(entry?.replacement ?? "");
    const [errors, setErrors] = useState<FieldErrors>({});
    const [saving, setSaving] = useState(false);

    async function save(event?: SyntheticEvent) {
        event?.preventDefault();
        const trimmedPhrase = phrase.trim();
        const trimmedReplacement = replacement.trim();
        if (trimmedPhrase.length === 0) {
            setErrors({ phrase: "Enter a word or phrase." });
            return;
        }
        if (isRule && trimmedReplacement.length === 0) {
            setErrors({ replacement: "Enter what it should be written as." });
            return;
        }
        setSaving(true);
        setErrors({});
        const written = isRule ? trimmedReplacement : null;
        try {
            const saved = entry
                ? await api.updateDictionaryEntry(entry.id, trimmedPhrase, written)
                : await api.addDictionaryEntry(trimmedPhrase, written);
            onSaved(saved, entry === null);
        } catch (error) {
            const message = errorMessage(error);
            // The backend names the offending field in its message; attach it there.
            setErrors(
                isRule && /replacement|written/i.test(message)
                    ? { replacement: message }
                    : { phrase: message },
            );
        } finally {
            setSaving(false);
        }
    }

    return (
        <Modal
            open
            onClose={onClose}
            title={entry ? "Edit entry" : "Add to dictionary"}
            width={460}
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
                        {entry ? "Save" : "Add"}
                    </Button>
                </>
            }
        >
            <form
                className="dictionary-form"
                onSubmit={(event) => {
                    void save(event);
                }}
            >
                <TextField
                    label={isRule ? "When I say" : "Word or phrase"}
                    placeholder={isRule ? "e.g. argo city" : "e.g. Kubernetes"}
                    value={phrase}
                    autoFocus
                    error={errors.phrase}
                    hint={
                        isRule
                            ? "Matched as whole words, ignoring case."
                            : "Simple Voice listens for it and keeps this exact spelling."
                    }
                    onChange={(event) => {
                        setPhrase(event.target.value);
                    }}
                />
                <label className="dictionary-form__toggle">
                    <span>
                        <span className="dictionary-form__toggle-title">
                            Replace what I say with…
                        </span>
                        <span className="dictionary-form__toggle-hint">
                            Write something different whenever this is heard.
                        </span>
                    </span>
                    <Toggle checked={isRule} onChange={setIsRule} label="Replace what I say" />
                </label>
                {isRule && (
                    <TextField
                        label="Write it as"
                        placeholder="e.g. ArgoCD"
                        value={replacement}
                        error={errors.replacement}
                        onChange={(event) => {
                            setReplacement(event.target.value);
                        }}
                    />
                )}
                {isRule && phrase.trim().length > 0 && replacement.trim().length > 0 && (
                    <div className="dictionary-form__preview">
                        <span className="caps-label">Preview</span>
                        <span className="dictionary-form__preview-line">
                            <span className="dictionary-row__heard">{phrase.trim()}</span>
                            <ArrowRight
                                className="dictionary-row__arrow"
                                aria-label="is written as"
                            />
                            <span className="dictionary-row__written">{replacement.trim()}</span>
                        </span>
                    </div>
                )}
                {/* Enter in either field submits. */}
                <button type="submit" className="sr-only" aria-hidden="true" tabIndex={-1} />
            </form>
        </Modal>
    );
}
