// FilePath: engine/Sources/SimpleVoiceEngine/ModelCatalog.swift
// What each downloadable model is made of and where it comes from: the Hugging Face files the
// loader needs, plus the licence, provenance, pinned revision and size shown before download.

import Foundation

/// The files of one Hugging Face repository folder that a local model needs.
struct HubSource: Sendable {
    let repo: String
    /// Repository folder holding the variant (empty for the repository root).
    let subdirectory: String
    /// Files or `.mlmodelc` directories, relative to `subdirectory`.
    let entries: [String]
    /// Branch or commit the files are listed and fetched from; a commit pins exactly what the
    /// catalog describes.
    var revision = "main"

    /// Maps a repository path to its path inside the model directory, or nil when not needed.
    func localPath(forRepoPath repoPath: String) -> String? {
        let prefix = subdirectory.isEmpty ? "" : subdirectory + "/"
        guard repoPath.hasPrefix(prefix) else { return nil }
        let relative = String(repoPath.dropFirst(prefix.count))
        let wanted = entries.contains { relative == $0 || relative.hasPrefix($0 + "/") }
        return wanted ? relative : nil
    }
}

/// Everything one local model needs, possibly gathered from several repositories. Every source
/// lands at the top of the same model directory, so their local paths must not overlap.
struct HubModel: Sendable {
    let id: ModelID
    let sources: [HubSource]
    /// What the user is told before downloading: licence and origin, plus the pinned revision and
    /// exact size where the catalog fixes them.
    let catalog: ModelCatalog

    static func forModel(_ id: ModelID) -> HubModel? {
        switch id {
        case .parakeetTdtV3:
            HubModel(
                id: id,
                sources: [
                    HubSource(
                        repo: "FluidInference/parakeet-tdt-0.6b-v3-coreml",
                        subdirectory: "",
                        entries: [
                            "Preprocessor.mlmodelc", "Encoder.mlmodelc", "Decoder.mlmodelc",
                            "JointDecisionv3.mlmodelc", "parakeet_vocab.json",
                        ]
                    )
                ],
                catalog: ModelCatalog(
                    license: "CC-BY-4.0",
                    source: "https://huggingface.co/FluidInference/parakeet-tdt-0.6b-v3-coreml "
                        + "(Core ML conversion of https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3)"
                ))
        case .parakeetTdtV2:
            HubModel(
                id: id,
                sources: [
                    HubSource(
                        repo: "FluidInference/parakeet-tdt-0.6b-v2-coreml",
                        subdirectory: "",
                        entries: [
                            "Preprocessor.mlmodelc", "Encoder.mlmodelc", "Decoder.mlmodelc",
                            "JointDecision.mlmodelc", "parakeet_vocab.json",
                        ]
                    )
                ],
                catalog: ModelCatalog(
                    license: "CC-BY-4.0",
                    source: "https://huggingface.co/FluidInference/parakeet-tdt-0.6b-v2-coreml "
                        + "(Core ML conversion of https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2)"
                ))
        case .parakeetFlash:
            // The 1280 ms variant: the helper transcribes whole recordings, where the largest
            // chunk gives the best accuracy and throughput.
            HubModel(
                id: id,
                sources: [
                    HubSource(
                        repo: "FluidInference/parakeet-realtime-eou-120m-coreml",
                        subdirectory: "1280ms",
                        entries: [
                            "streaming_encoder.mlmodelc", "decoder.mlmodelc", "joint_decision.mlmodelc", "vocab.json",
                        ]
                    )
                ],
                catalog: ModelCatalog(
                    license: "NVIDIA Open Model License "
                        + "(https://www.nvidia.com/en-us/agreements/enterprise-software/nvidia-open-model-license/)",
                    source: "https://huggingface.co/FluidInference/parakeet-realtime-eou-120m-coreml "
                        + "(Core ML conversion of https://huggingface.co/nvidia/parakeet_realtime_eou_120m-v1)"
                ))
        case .whisperLargeV3Turbo:
            HubModel(
                id: id,
                sources: [
                    HubSource(
                        repo: "argmaxinc/whisperkit-coreml",
                        subdirectory: "openai_whisper-large-v3-v20240930_turbo",
                        entries: [
                            "AudioEncoder.mlmodelc", "MelSpectrogram.mlmodelc", "TextDecoder.mlmodelc",
                            "TextDecoderContextPrefill.mlmodelc", "config.json", "generation_config.json",
                        ]
                    ),
                    whisperTokenizer,
                ],
                catalog: ModelCatalog(
                    license: "MIT (Core ML conversion); tokenizer Apache-2.0",
                    source: "https://huggingface.co/argmaxinc/whisperkit-coreml "
                        + "(openai_whisper-large-v3-v20240930_turbo); tokenizer https://huggingface.co/openai/whisper-large-v3"
                ))
        case .whisperHinglish:
            HubModel(
                id: id,
                sources: [
                    HubSource(
                        repo: "vbhar/whisperkit-hindi2hinglish-prime-coreml",
                        subdirectory: "Oriserve_Whisper-Hindi2Hinglish-Prime_fp16",
                        entries: [
                            "AudioEncoder.mlmodelc", "MelSpectrogram.mlmodelc", "TextDecoder.mlmodelc",
                            "config.json", "generation_config.json",
                        ]
                    ),
                    whisperTokenizer,
                ],
                catalog: ModelCatalog(
                    license: "Apache-2.0",
                    source: "https://huggingface.co/vbhar/whisperkit-hindi2hinglish-prime-coreml "
                        + "(Core ML conversion of https://huggingface.co/Oriserve/Whisper-Hindi2Hinglish-Prime); "
                        + "tokenizer https://huggingface.co/openai/whisper-large-v3"
                ))
        case .qwen3Instruct:
            HubModel(
                id: id,
                sources: [
                    HubSource(
                        repo: "unsloth/Qwen3-4B-Instruct-2507-GGUF",
                        subdirectory: "",
                        entries: [LocalLanguageModel.qwenFile],
                        revision: "a06e946bb6b655725eafa393f4a9745d460374c9"
                    )
                ],
                catalog: ModelCatalog(
                    license: "Apache-2.0",
                    source: "https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF "
                        + "(Q4_K_M quantization of https://huggingface.co/Qwen/Qwen3-4B-Instruct-2507)",
                    revision: "a06e946bb6b655725eafa393f4a9745d460374c9",
                    downloadBytes: 2_497_281_120
                ))
        case .speakerDiarization:
            HubModel(
                id: id,
                sources: [
                    HubSource(
                        repo: "FluidInference/speaker-diarization-coreml",
                        subdirectory: "",
                        entries: SpeakerDiarizer.requiredEntries,
                        revision: "7eab74f3463c736418d0eb10296261ae581146c9"
                    )
                ],
                catalog: ModelCatalog(
                    license: "CC-BY-4.0 (pyannote speaker-diarization-community-1, WeSpeaker, "
                        + "BUT Speech@FIT PLDA; Core ML conversion by Fluid Inference)",
                    source: "https://huggingface.co/FluidInference/speaker-diarization-coreml",
                    revision: "7eab74f3463c736418d0eb10296261ae581146c9",
                    downloadBytes: 21_599_417
                ))
        case .madlad:
            // GGUF-my-repo (llama.cpp convert_hf_to_gguf + Q4_K_M) conversion of Google's
            // checkpoint; a second repository holds a byte-identical file.
            HubModel(
                id: id,
                sources: [
                    HubSource(
                        repo: "mtsdurica/madlad400-3b-mt-Q4_K_M-GGUF",
                        subdirectory: "",
                        entries: [LocalTranslator.madladFile],
                        revision: "7f55e827c4c2fd1fdc52893879e18966d2867dba"
                    )
                ],
                catalog: ModelCatalog(
                    license: "Apache-2.0",
                    source: "https://huggingface.co/mtsdurica/madlad400-3b-mt-Q4_K_M-GGUF "
                        + "(Q4_K_M quantization of https://huggingface.co/google/madlad400-3b-mt)",
                    revision: "7f55e827c4c2fd1fdc52893879e18966d2867dba",
                    downloadBytes: 1_858_124_864
                ))
        case .parakeetVocabulary:
            HubModel(
                id: id,
                sources: [
                    HubSource(
                        repo: "FluidInference/parakeet-ctc-110m-coreml",
                        subdirectory: "",
                        entries: VocabularyBooster.requiredEntries,
                        revision: "accdafd8cf8a2ff1cabe3c11e54416b405d409aa"
                    )
                ],
                catalog: ModelCatalog(
                    license: "CC-BY-4.0",
                    source: "https://huggingface.co/FluidInference/parakeet-ctc-110m-coreml "
                        + "(Core ML conversion of https://huggingface.co/nvidia/parakeet-tdt_ctc-110m)",
                    revision: "accdafd8cf8a2ff1cabe3c11e54416b405d409aa",
                    downloadBytes: 102_802_455
                ))
        case .appleSpeech, .appleIntelligence:
            nil
        }
    }

    /// WhisperKit looks for `tokenizer.json` at the top of the model folder before it would
    /// fetch one, so bundling the large-v3 tokenizer keeps loading offline.
    private static let whisperTokenizer = HubSource(
        repo: "openai/whisper-large-v3",
        subdirectory: "",
        entries: ["tokenizer.json", "tokenizer_config.json"]
    )
}

/// The download facts of a catalog model, reported by `model_status` so the user sees licence,
/// origin and size before choosing to download. Older entries follow the repository's `main`
/// branch, whose exact size is only known once listed, so they carry neither.
struct ModelCatalog: Sendable {
    let license: String
    let source: String
    var revision: String? = nil
    var downloadBytes: Int64? = nil
}
