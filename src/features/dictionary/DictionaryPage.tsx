// FilePath: src/features/dictionary/DictionaryPage.tsx
import { useCallback, useEffect, useMemo, useState } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { ArrowRight, BookA, Pencil, Plus, Search, SearchX, Trash2, Upload } from "lucide-react";
import { api, errorMessage, type DictionaryEntry } from "../../lib/api";
import { formatNumber, pluralize } from "../../lib/format";
import {
    Button,
    EmptyState,
    Hero,
    IconButton,
    Modal,
    Spinner,
    TextField,
    useToast,
} from "../../ui";
import { DictionaryEditor } from "./DictionaryEditor";
import "./dictionary.css";

type EditorState = { entry: DictionaryEntry | null; key: number } | null;

const HERO_SUBTITLE =
    "Add names, jargon and brand spellings so they are recognised and written correctly. " +
    "Replacement rules turn what you say into exactly what you want written, in every app.";
const EMPTY_DESCRIPTION =
    "Add a word you use often, or import a vocabulary file with one entry per line. Lines " +
    "written as “heard -> written” become replacement rules.";

export function DictionaryPage() {
    const { toast } = useToast();
    const [entries, setEntries] = useState<DictionaryEntry[] | null>(null);
    const [filter, setFilter] = useState("");
    const [editor, setEditor] = useState<EditorState>(null);
    const [pendingDelete, setPendingDelete] = useState<DictionaryEntry | null>(null);
    const [deleting, setDeleting] = useState(false);
    const [importing, setImporting] = useState(false);

    const load = useCallback(
        () =>
            api.listDictionary().then(setEntries, (error: unknown) => {
                setEntries((current) => current ?? []);
                toast(errorMessage(error), "danger");
            }),
        [toast],
    );

    useEffect(() => {
        void load();
    }, [load]);

    const sorted = useMemo(
        () =>
            [...(entries ?? [])].sort((a, b) =>
                a.phrase.localeCompare(b.phrase, undefined, { sensitivity: "base" }),
            ),
        [entries],
    );

    const visible = useMemo(() => {
        const needle = filter.trim().toLowerCase();
        if (needle.length === 0) return sorted;
        return sorted.filter(
            (entry) =>
                entry.phrase.toLowerCase().includes(needle) ||
                (entry.replacement ?? "").toLowerCase().includes(needle),
        );
    }, [sorted, filter]);

    const ruleCount = sorted.filter((entry) => entry.replacement !== null).length;
    const wordCount = sorted.length - ruleCount;

    function openEditor(entry: DictionaryEntry | null) {
        setEditor({ entry, key: Date.now() });
    }

    async function importFile() {
        try {
            const path = await openFileDialog({
                multiple: false,
                directory: false,
                filters: [{ name: "Text", extensions: ["txt"] }],
            });
            if (typeof path !== "string") return;
            setImporting(true);
            const summary = await api.importVocabulary(path);
            await load();
            const noun = pluralize(summary.added, "entry", "entries");
            const added = `Imported ${formatNumber(summary.added)} ${noun}`;
            const skipped = `, skipped ${formatNumber(summary.skipped)} already in your dictionary`;
            toast(summary.skipped > 0 ? added + skipped : added, "success");
        } catch (error) {
            toast(errorMessage(error), "danger");
        } finally {
            setImporting(false);
        }
    }

    async function confirmDelete() {
        if (!pendingDelete) return;
        setDeleting(true);
        try {
            await api.deleteDictionaryEntry(pendingDelete.id);
            const removed = pendingDelete.id;
            setEntries((current) => (current ?? []).filter((entry) => entry.id !== removed));
            setPendingDelete(null);
            toast(`Removed “${pendingDelete.phrase}”`);
        } catch (error) {
            toast(errorMessage(error), "danger");
        } finally {
            setDeleting(false);
        }
    }

    return (
        <>
            <Hero
                title={
                    <>
                        Simple Voice spells the way <em>you</em> do.
                    </>
                }
                subtitle={HERO_SUBTITLE}
            >
                <Button
                    variant="primary"
                    icon={<Plus />}
                    onClick={() => {
                        openEditor(null);
                    }}
                >
                    Add word
                </Button>
                <Button
                    variant="secondary"
                    icon={<Upload />}
                    loading={importing}
                    onClick={() => {
                        void importFile();
                    }}
                >
                    Import…
                </Button>
            </Hero>

            <div className="dictionary-toolbar">
                <div className="dictionary-search">
                    <Search className="dictionary-search__icon" aria-hidden="true" />
                    <TextField
                        type="search"
                        placeholder="Filter dictionary"
                        aria-label="Filter dictionary"
                        value={filter}
                        onChange={(event) => {
                            setFilter(event.target.value);
                        }}
                    />
                </div>
                {entries !== null && entries.length > 0 && (
                    <span className="dictionary-count">
                        {formatNumber(wordCount)} {pluralize(wordCount, "word")} ·{" "}
                        {formatNumber(ruleCount)} {pluralize(ruleCount, "rule")}
                    </span>
                )}
            </div>

            {entries === null ? (
                <div className="dictionary-loading">
                    <Spinner size={20} />
                </div>
            ) : entries.length === 0 ? (
                <EmptyState
                    icon={<BookA />}
                    title="Your dictionary is empty"
                    description={EMPTY_DESCRIPTION}
                    action={
                        <Button
                            variant="primary"
                            icon={<Plus />}
                            onClick={() => {
                                openEditor(null);
                            }}
                        >
                            Add your first word
                        </Button>
                    }
                />
            ) : visible.length === 0 ? (
                <EmptyState
                    icon={<SearchX />}
                    title="No matches"
                    description={`Nothing in your dictionary matches “${filter.trim()}”.`}
                />
            ) : (
                <ul className="dictionary-list">
                    {visible.map((entry) => (
                        <li key={entry.id} className="dictionary-row">
                            {entry.replacement === null ? (
                                <span className="dictionary-row__phrase selectable">
                                    {entry.phrase}
                                </span>
                            ) : (
                                <span className="dictionary-row__rule">
                                    <span className="dictionary-row__heard selectable">
                                        {entry.phrase}
                                    </span>
                                    <ArrowRight
                                        className="dictionary-row__arrow"
                                        aria-label="is written as"
                                    />
                                    <span className="dictionary-row__phrase selectable">
                                        {entry.replacement}
                                    </span>
                                </span>
                            )}
                            <span className="dictionary-row__actions">
                                <IconButton
                                    label={`Edit “${entry.phrase}”`}
                                    icon={<Pencil />}
                                    onClick={() => {
                                        openEditor(entry);
                                    }}
                                />
                                <IconButton
                                    label={`Delete “${entry.phrase}”`}
                                    icon={<Trash2 />}
                                    onClick={() => {
                                        setPendingDelete(entry);
                                    }}
                                />
                            </span>
                        </li>
                    ))}
                </ul>
            )}

            {editor && (
                <DictionaryEditor
                    key={editor.key}
                    entry={editor.entry}
                    onClose={() => {
                        setEditor(null);
                    }}
                    onSaved={(saved, created) => {
                        setEntries((current) => {
                            const list = current ?? [];
                            return created
                                ? [...list, saved]
                                : list.map((item) => (item.id === saved.id ? saved : item));
                        });
                        setEditor(null);
                        toast(created ? `Added “${saved.phrase}”` : "Saved", "success");
                    }}
                />
            )}

            <Modal
                open={pendingDelete !== null}
                onClose={() => {
                    if (!deleting) setPendingDelete(null);
                }}
                title="Remove from dictionary?"
                width={420}
                footer={
                    <>
                        <Button
                            variant="ghost"
                            disabled={deleting}
                            onClick={() => {
                                setPendingDelete(null);
                            }}
                        >
                            Cancel
                        </Button>
                        <Button
                            variant="danger"
                            loading={deleting}
                            onClick={() => {
                                void confirmDelete();
                            }}
                        >
                            Remove
                        </Button>
                    </>
                }
            >
                <p className="dictionary-confirm">
                    {pendingDelete?.replacement == null
                        ? `“${pendingDelete?.phrase ?? ""}” will no longer guide recognition.`
                        : `“${pendingDelete.phrase}” will no longer be rewritten as ` +
                          `“${pendingDelete.replacement}”.`}
                </p>
            </Modal>
        </>
    );
}
