// FilePath: src/features/settings/models/useModels.ts
// Model catalogue with live download progress. Choosing a model that is not on disk yet downloads
// it first and only switches to it once the files are ready, so dictation never points at a model
// that cannot load. A voice model becomes `transcriptionModel`, a local language model becomes
// `localLlmModel`; the speaker model is not a choice, only a download.
import { useCallback, useEffect, useRef, useState } from "react";
import {
    api,
    errorMessage,
    events,
    type ModelInfo,
    type ModelProgress,
    type SettingsPatch,
} from "../../../lib/api";
import { useTauriEvent } from "../../../lib/useTauriEvent";
import { useSettings } from "../../../app/SettingsContext";

/** The setting a model is chosen into; null when the model is not a choice. */
function choicePatch(model: ModelInfo): SettingsPatch | null {
    if (model.kind === "transcription") return { transcriptionModel: model.id };
    if (model.kind === "post_processing" && model.provider === "llm") {
        return { localLlmModel: model.id };
    }
    return null;
}

interface PendingUse {
    id: string;
    patch: SettingsPatch;
}

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
    /** Forget the pending choice; its download keeps running but no longer switches models. */
    cancelPendingUse: () => void;
}

export function useModels(): ModelsState {
    const { update } = useSettings();
    const [models, setModels] = useState<ModelInfo[] | null>(null);
    const [loadError, setLoadError] = useState<string | null>(null);
    const [progress, setProgress] = useState<Partial<Record<string, number>>>({});
    const [failures, setFailures] = useState<Partial<Record<string, string>>>({});
    const [pendingUse, setPendingUse] = useState<string | null>(null);
    const pendingRef = useRef<PendingUse | null>(null);

    const refresh = useCallback(async () => {
        try {
            const list = await api.listModels();
            setModels(list);
            setLoadError(null);
            const pending = pendingRef.current;
            if (pending !== null && list.some((m) => m.id === pending.id && m.status === "ready")) {
                pendingRef.current = null;
                setPendingUse(null);
                await update(pending.patch);
            }
        } catch (e) {
            setLoadError(errorMessage(e));
        }
    }, [update]);

    useEffect(() => {
        void refresh();
    }, [refresh]);

    const clearPending = useCallback((id: string) => {
        if (pendingRef.current?.id === id) {
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
    // A delete, or a download finished from another page, changes what is on disk.
    useTauriEvent(events.modelsChanged, () => {
        void refresh();
    });

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
            const patch = choicePatch(model);
            if (patch === null || model.status === "unsupported") return;
            if (model.status === "not_downloaded" || model.status === "downloading") {
                pendingRef.current = { id: model.id, patch };
                setPendingUse(model.id);
                if (model.status === "not_downloaded") await download(model.id);
                return;
            }
            await update(patch);
        },
        [download, update],
    );

    const cancelPendingUse = useCallback(() => {
        pendingRef.current = null;
        setPendingUse(null);
    }, []);

    return {
        models,
        loadError,
        progress,
        failures,
        pendingUse,
        refresh,
        download,
        remove,
        use,
        cancelPendingUse,
    };
}
