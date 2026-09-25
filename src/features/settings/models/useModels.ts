// FilePath: src/features/settings/models/useModels.ts
// Model catalogue with live download progress. Choosing a local model that is not on disk yet
// downloads it first and only switches to it once the files are ready, so dictation never points
// at a model that cannot load.
import { useCallback, useEffect, useRef, useState } from "react";
import { api, errorMessage, events, type ModelInfo, type ModelProgress } from "../../../lib/api";
import { useTauriEvent } from "../../../lib/useTauriEvent";
import { useSettings } from "../../../app/SettingsContext";

export interface ModelsState {
    models: ModelInfo[] | null;
    loadError: string | null;
    /** Download fraction per model id while a download runs. */
    progress: Partial<Record<string, number>>;
    failures: Partial<Record<string, string>>;
    /** Model that becomes active once its download finishes. */
    pendingUse: string | null;
    refresh: () => Promise<void>;
    download: (id: string) => Promise<void>;
    remove: (id: string) => Promise<void>;
    use: (model: ModelInfo) => Promise<void>;
}

export function useModels(): ModelsState {
    const { update } = useSettings();
    const [models, setModels] = useState<ModelInfo[] | null>(null);
    const [loadError, setLoadError] = useState<string | null>(null);
    const [progress, setProgress] = useState<Partial<Record<string, number>>>({});
    const [failures, setFailures] = useState<Partial<Record<string, string>>>({});
    const [pendingUse, setPendingUse] = useState<string | null>(null);
    const pendingRef = useRef<string | null>(null);

    const refresh = useCallback(async () => {
        try {
            const list = await api.listModels();
            setModels(list);
            setLoadError(null);
            const pending = pendingRef.current;
            if (pending !== null && list.some((m) => m.id === pending && m.status === "ready")) {
                pendingRef.current = null;
                setPendingUse(null);
                await update({ transcriptionModel: pending });
            }
        } catch (e) {
            setLoadError(errorMessage(e));
        }
    }, [update]);

    useEffect(() => {
        void refresh();
    }, [refresh]);

    const clearPending = useCallback((id: string) => {
        if (pendingRef.current === id) {
            pendingRef.current = null;
            setPendingUse(null);
        }
    }, []);

    const onProgress = useCallback(
        (p: ModelProgress) => {
            if (p.status === "downloading") {
                setProgress((prev) => ({ ...prev, [p.id]: p.fraction }));
                return;
            }
            setProgress((prev) => ({ ...prev, [p.id]: undefined }));
            if (p.status === "failed") {
                setFailures((prev) => ({ ...prev, [p.id]: p.message ?? "Download failed." }));
                clearPending(p.id);
            }
            void refresh();
        },
        [refresh, clearPending],
    );
    useTauriEvent(events.modelProgress, onProgress);

    const download = useCallback(
        async (id: string) => {
            setFailures((prev) => ({ ...prev, [id]: undefined }));
            setProgress((prev) => ({ ...prev, [id]: prev[id] ?? 0 }));
            try {
                await api.downloadModel(id);
            } catch (e) {
                setFailures((prev) => ({ ...prev, [id]: errorMessage(e) }));
                setProgress((prev) => ({ ...prev, [id]: undefined }));
                clearPending(id);
            }
            await refresh();
        },
        [refresh, clearPending],
    );

    const remove = useCallback(
        async (id: string) => {
            setFailures((prev) => ({ ...prev, [id]: undefined }));
            try {
                await api.deleteModel(id);
            } catch (e) {
                setFailures((prev) => ({ ...prev, [id]: errorMessage(e) }));
            }
            await refresh();
        },
        [refresh],
    );

    const use = useCallback(
        async (model: ModelInfo) => {
            if (model.status === "unsupported") return;
            if (model.status === "not_downloaded" || model.status === "downloading") {
                pendingRef.current = model.id;
                setPendingUse(model.id);
                if (model.status === "not_downloaded") await download(model.id);
                return;
            }
            await update({ transcriptionModel: model.id });
        },
        [download, update],
    );

    return { models, loadError, progress, failures, pendingUse, refresh, download, remove, use };
}
