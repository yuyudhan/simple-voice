// FilePath: src/features/dictionary/BuiltInVocabulary.tsx
// The built-in vocabulary packs: curated spellings for kinds of work, switched on beside the
// personal dictionary. The packs and their words come from the backend, so adding one there
// adds it here.
import { useEffect, useState } from "react";
import { ChevronDown } from "lucide-react";
import { useSettings } from "../../app/SettingsContext";
import { api, errorMessage, type VocabularyPack, type VocabularyPackInfo } from "../../lib/api";
import { formatNumber, pluralize } from "../../lib/format";
import { Button, Modal, Spinner, Toggle, useToast } from "../../ui";
import "./builtin.css";

const INTRO =
    "Pick the kinds of work you talk about. Every term in a pack is spelled the way it is " +
    "written; its first few also guide recognition, after your own words, which always win. " +
    "Simple Voice's own name is always included.";

export interface BuiltInVocabularyProps {
    open: boolean;
    onClose: () => void;
}

export function BuiltInVocabulary({ open, onClose }: BuiltInVocabularyProps) {
    const { settings, update } = useSettings();
    const { toast } = useToast();
    const [packs, setPacks] = useState<VocabularyPackInfo[] | null>(null);
    const [expanded, setExpanded] = useState<VocabularyPack | null>(null);
    const enabled = settings.vocabularyPacks;

    useEffect(() => {
        if (!open || packs !== null) return;
        api.listVocabularyPacks().then(setPacks, (error: unknown) => {
            setPacks([]);
            toast(errorMessage(error), "danger");
        });
    }, [open, packs, toast]);

    function setEnabled(id: VocabularyPack, on: boolean) {
        const next = on ? [...enabled, id] : enabled.filter((pack) => pack !== id);
        void update({ vocabularyPacks: next });
    }

    return (
        <Modal
            open={open}
            onClose={onClose}
            title="Built-in vocabulary"
            width={560}
            footer={
                <Button variant="primary" onClick={onClose}>
                    Done
                </Button>
            }
        >
            <p className="dictionary-packs__intro">{INTRO}</p>
            {packs === null ? (
                <div className="dictionary-loading">
                    <Spinner size={20} />
                </div>
            ) : (
                <ul className="dictionary-packs">
                    {packs.map((pack) => {
                        const isExpanded = expanded === pack.id;
                        const count = pack.terms.length;
                        return (
                            <li key={pack.id} className="dictionary-pack">
                                <div className="dictionary-pack__row">
                                    <div className="dictionary-pack__text">
                                        <span className="dictionary-pack__name">{pack.name}</span>
                                        <span className="dictionary-pack__description">
                                            {pack.description}
                                        </span>
                                        <button
                                            type="button"
                                            className={
                                                isExpanded
                                                    ? "dictionary-pack__more is-open"
                                                    : "dictionary-pack__more"
                                            }
                                            aria-expanded={isExpanded}
                                            onClick={() => {
                                                setExpanded(isExpanded ? null : pack.id);
                                            }}
                                        >
                                            {formatNumber(count)} {pluralize(count, "term")}
                                            <ChevronDown aria-hidden="true" />
                                        </button>
                                    </div>
                                    <Toggle
                                        label={`Use ${pack.name}`}
                                        checked={enabled.includes(pack.id)}
                                        onChange={(on) => {
                                            setEnabled(pack.id, on);
                                        }}
                                    />
                                </div>
                                {isExpanded && (
                                    <p className="dictionary-pack__terms selectable">
                                        {pack.terms.join(", ")}
                                    </p>
                                )}
                            </li>
                        );
                    })}
                </ul>
            )}
        </Modal>
    );
}
