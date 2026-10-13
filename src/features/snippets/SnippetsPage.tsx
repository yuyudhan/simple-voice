// FilePath: src/features/snippets/SnippetsPage.tsx
// Personal snippets: say a short trigger while dictating and its saved text is inserted in its
// place, exactly as written. Stored only in the local database.
import { useCallback, useEffect, useMemo, useState } from "react";
import { ArrowRight, Pencil, Plus, Scissors, Search, SearchX, Trash2 } from "lucide-react";
import { api, errorMessage, events, type Snippet } from "../../lib/api";
import { useTauriEvent } from "../../lib/useTauriEvent";
import { dayLabel, formatLongDate } from "../../lib/format";
import {
    Button,
    EmptyState,
    IconButton,
    Modal,
    PageHeader,
    Spinner,
    TextField,
    useToast,
} from "../../ui";
import { SnippetEditor } from "./SnippetEditor";
import "./snippets.css";

const DESCRIPTION =
    "Say a trigger while dictating and Simple Voice writes its saved text instead, line breaks " +
    "and all. Snippets stay on this Mac.";
const EMPTY_DESCRIPTION =
    "Save text you type often, like your address, a sign-off or a meeting link, and insert it " +
    "by saying a short phrase.";

type EditorState = { snippet: Snippet | null; key: number } | null;

export function SnippetsPage() {
    const { toast } = useToast();
    const [snippets, setSnippets] = useState<Snippet[] | null>(null);
    const [filter, setFilter] = useState("");
    const [editor, setEditor] = useState<EditorState>(null);
    const [pendingDelete, setPendingDelete] = useState<Snippet | null>(null);
    const [deleting, setDeleting] = useState(false);

    const load = useCallback(
        () =>
            api.listSnippets().then(setSnippets, (error: unknown) => {
                setSnippets((current) => current ?? []);
                toast(errorMessage(error), "danger");
            }),
        [toast],
    );

    useEffect(() => {
        void load();
    }, [load]);

    useTauriEvent(events.snippetsChanged, () => {
        void load();
    });

    const visible = useMemo(() => {
        const sorted = [...(snippets ?? [])].sort((a, b) =>
            a.trigger.localeCompare(b.trigger, undefined, { sensitivity: "base" }),
        );
        const needle = filter.trim().toLowerCase();
        if (needle.length === 0) return sorted;
        return sorted.filter(
            (snippet) =>
                snippet.trigger.toLowerCase().includes(needle) ||
                snippet.text.toLowerCase().includes(needle),
        );
    }, [snippets, filter]);

    const now = new Date();

    async function confirmDelete() {
        if (!pendingDelete) return;
        setDeleting(true);
        try {
            await api.deleteSnippet(pendingDelete.id);
            const removed = pendingDelete.id;
            setSnippets((current) => (current ?? []).filter((s) => s.id !== removed));
            toast(`Removed “${pendingDelete.trigger}”`);
            setPendingDelete(null);
        } catch (error) {
            toast(errorMessage(error), "danger");
        } finally {
            setDeleting(false);
        }
    }

    const openEditor = (snippet: Snippet | null) => {
        setEditor({ snippet, key: Date.now() });
    };

    return (
        <>
            <PageHeader
                title="Snippets"
                description={DESCRIPTION}
                actions={
                    <Button
                        variant="primary"
                        icon={<Plus />}
                        onClick={() => {
                            openEditor(null);
                        }}
                    >
                        New snippet
                    </Button>
                }
            />

            {snippets !== null && snippets.length > 0 && (
                <div className="snippets-toolbar">
                    <div className="snippets-search">
                        <Search className="snippets-search__icon" aria-hidden="true" />
                        <TextField
                            type="search"
                            placeholder="Filter snippets"
                            aria-label="Filter snippets"
                            value={filter}
                            onChange={(event) => {
                                setFilter(event.target.value);
                            }}
                        />
                    </div>
                </div>
            )}

            {snippets === null ? (
                <div className="snippets-loading">
                    <Spinner size={20} />
                </div>
            ) : snippets.length === 0 ? (
                <EmptyState
                    icon={<Scissors />}
                    title="No snippets yet"
                    description={EMPTY_DESCRIPTION}
                    action={
                        <Button
                            variant="primary"
                            icon={<Plus />}
                            onClick={() => {
                                openEditor(null);
                            }}
                        >
                            Add your first snippet
                        </Button>
                    }
                />
            ) : visible.length === 0 ? (
                <EmptyState
                    icon={<SearchX />}
                    title="No matches"
                    description={`No snippet matches “${filter.trim()}”.`}
                />
            ) : (
                <ul className="snippets-list">
                    {visible.map((snippet) => (
                        <li key={snippet.id} className="snippet-row">
                            <div className="snippet-row__main">
                                <span className="snippet-row__trigger selectable">
                                    {snippet.trigger}
                                </span>
                                <ArrowRight className="snippet-row__arrow" aria-label="inserts" />
                                <pre className="snippet-row__text selectable">{snippet.text}</pre>
                            </div>
                            <span
                                className="snippet-row__date"
                                title={`Updated ${formatLongDate(new Date(snippet.updatedAt))}`}
                            >
                                {dayLabel(new Date(snippet.updatedAt), now, { weekday: false })}
                            </span>
                            <span className="snippet-row__actions">
                                <IconButton
                                    label={`Edit “${snippet.trigger}”`}
                                    icon={<Pencil />}
                                    onClick={() => {
                                        openEditor(snippet);
                                    }}
                                />
                                <IconButton
                                    label={`Delete “${snippet.trigger}”`}
                                    icon={<Trash2 />}
                                    onClick={() => {
                                        setPendingDelete(snippet);
                                    }}
                                />
                            </span>
                        </li>
                    ))}
                </ul>
            )}

            {editor && (
                <SnippetEditor
                    key={editor.key}
                    snippet={editor.snippet}
                    onClose={() => {
                        setEditor(null);
                    }}
                    onSaved={(saved, created) => {
                        setSnippets((current) => {
                            const list = current ?? [];
                            return created
                                ? [...list, saved]
                                : list.map((item) => (item.id === saved.id ? saved : item));
                        });
                        setEditor(null);
                        toast(created ? `Added “${saved.trigger}”` : "Saved", "success");
                    }}
                />
            )}

            <Modal
                open={pendingDelete !== null}
                onClose={() => {
                    if (!deleting) setPendingDelete(null);
                }}
                title="Delete snippet?"
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
                            Delete
                        </Button>
                    </>
                }
            >
                <p className="snippets-confirm">
                    Saying “{pendingDelete?.trigger ?? ""}” will no longer insert its text.
                </p>
            </Modal>
        </>
    );
}
