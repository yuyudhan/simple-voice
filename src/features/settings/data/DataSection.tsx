// FilePath: src/features/settings/data/DataSection.tsx
import { useState } from "react";
import { FolderOpen } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api, errorMessage, type AppInfo } from "../../../lib/api";
import { useSettings } from "../../../app/SettingsContext";
import { Button, Modal, SettingRow, SettingsGroup, useToast } from "../../../ui";
import "../common.css";
import "./data.css";

interface PathRowProps {
    title: string;
    description: string;
    path: string;
}

function PathRow({ title, description, path }: PathRowProps) {
    const { toast } = useToast();
    return (
        <SettingRow title={title} description={description}>
            <div className="sv-path">
                <code className="sv-path__value" title={path}>
                    {path}
                </code>
                <Button
                    size="sm"
                    variant="ghost"
                    icon={<FolderOpen size={14} />}
                    onClick={() => {
                        void revealItemInDir(path).catch((e: unknown) => {
                            toast(errorMessage(e), "danger");
                        });
                    }}
                >
                    Reveal
                </Button>
            </div>
        </SettingRow>
    );
}

interface Props {
    info: AppInfo | null;
    onInfoChanged: () => Promise<void>;
}

export function DataSection({ info, onInfoChanged }: Props) {
    const { settings, refresh } = useSettings();
    const { toast } = useToast();
    const [targetDir, setTargetDir] = useState<string | null>(null);
    const [moving, setMoving] = useState(false);
    const [moveError, setMoveError] = useState<string | null>(null);
    const [confirmClear, setConfirmClear] = useState(false);
    const [clearing, setClearing] = useState(false);

    const chooseDir = async () => {
        try {
            const dir = await open({
                directory: true,
                multiple: false,
                defaultPath: settings.databaseDir,
                title: "Choose a folder for the Simple Voice database",
            });
            if (dir !== null && dir !== settings.databaseDir) {
                setMoveError(null);
                setTargetDir(dir);
            }
        } catch (e) {
            toast(errorMessage(e), "danger");
        }
    };

    const move = async () => {
        if (targetDir === null) return;
        setMoving(true);
        setMoveError(null);
        try {
            await api.setDatabaseDir(targetDir);
            await refresh();
            await onInfoChanged();
            setTargetDir(null);
            toast("Database moved.", "success");
        } catch (e) {
            setMoveError(errorMessage(e));
        } finally {
            setMoving(false);
        }
    };

    const clear = async () => {
        setClearing(true);
        try {
            await api.clearHistory();
            setConfirmClear(false);
            toast("History cleared.", "success");
        } catch (e) {
            toast(errorMessage(e), "danger");
        } finally {
            setClearing(false);
        }
    };

    const databasePath = info?.databasePath ?? settings.databaseDir;

    return (
        <>
            <SettingsGroup title="Storage">
                <SettingRow
                    title="Database location"
                    description="Your settings, history and dictionary live in one SQLite file in this folder."
                >
                    <div className="sv-path">
                        <code className="sv-path__value" title={databasePath}>
                            {settings.databaseDir}
                        </code>
                        <Button
                            size="sm"
                            variant="secondary"
                            onClick={() => {
                                void chooseDir();
                            }}
                        >
                            Change…
                        </Button>
                        <Button
                            size="sm"
                            variant="ghost"
                            icon={<FolderOpen size={14} />}
                            onClick={() => {
                                void revealItemInDir(databasePath).catch((e: unknown) => {
                                    toast(errorMessage(e), "danger");
                                });
                            }}
                        >
                            Reveal
                        </Button>
                    </div>
                </SettingRow>
                {info && (
                    <>
                        <PathRow
                            title="Data folder"
                            description="Backups taken before upgrades and audio kept for retrying failed dictations."
                            path={info.dataDir}
                        />
                        <PathRow
                            title="Models folder"
                            description="Downloaded local voice models."
                            path={info.modelsDir}
                        />
                    </>
                )}
            </SettingsGroup>

            <SettingsGroup title="History">
                <SettingRow
                    title="Clear history"
                    description="Delete every dictation and its saved audio. Your dictionary and settings stay."
                >
                    <Button
                        size="sm"
                        variant="danger"
                        onClick={() => {
                            setConfirmClear(true);
                        }}
                    >
                        Clear history
                    </Button>
                </SettingRow>
            </SettingsGroup>

            <Modal
                open={targetDir !== null}
                onClose={() => {
                    if (!moving) setTargetDir(null);
                }}
                title="Move the database?"
                width={460}
                footer={
                    <>
                        <Button
                            variant="ghost"
                            disabled={moving}
                            onClick={() => {
                                setTargetDir(null);
                            }}
                        >
                            Cancel
                        </Button>
                        <Button
                            variant="primary"
                            loading={moving}
                            onClick={() => {
                                void move();
                            }}
                        >
                            Move database
                        </Button>
                    </>
                }
            >
                <div className="sv-data__confirm">
                    <p>
                        Simple Voice will copy your settings, history and dictionary into this
                        folder, switch to the copy, and remove the old file. It remembers the new
                        location across restarts.
                    </p>
                    <code className="sv-path__value sv-path__value--block">{targetDir}</code>
                    <p className="sv-inline-note">
                        Choose a local folder that is always available when Simple Voice runs.
                    </p>
                    {moveError && (
                        <p className="sv-inline-error" role="alert">
                            {moveError}
                        </p>
                    )}
                </div>
            </Modal>

            <Modal
                open={confirmClear}
                onClose={() => {
                    if (!clearing) setConfirmClear(false);
                }}
                title="Clear all history?"
                width={420}
                footer={
                    <>
                        <Button
                            variant="ghost"
                            disabled={clearing}
                            onClick={() => {
                                setConfirmClear(false);
                            }}
                        >
                            Cancel
                        </Button>
                        <Button
                            variant="danger"
                            loading={clearing}
                            onClick={() => {
                                void clear();
                            }}
                        >
                            Clear history
                        </Button>
                    </>
                }
            >
                <p className="sv-data__confirm">
                    Every dictation and its saved audio will be deleted. Insights start again from
                    zero. This cannot be undone.
                </p>
            </Modal>
        </>
    );
}
