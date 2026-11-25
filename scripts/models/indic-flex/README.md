<!-- FilePath: scripts/models/indic-flex/README.md -->

# indic-flex: Indic-Transcribe-Flex → FluidAudio Canary Core ML int4 → Hugging Face

Simple Voice's `indic-hinglish` voice model is built here. The script converts
`bodhan-ai/indic-transcribe-flex` at one upstream revision into the int4 Core ML layout that FluidAudio
0.17.4's Canary loader reads. It then validates the result against NeMo and publishes it to
`simple-voice/indic-transcribe-flex-coreml`, public and ungated, tagged `flex-<sha12>`.

## Usage

```sh
scripts/models/indic-flex/publish.sh <upstream-flex-revision-sha> [options]
# light checks only, with no model load:
scripts/models/indic-flex/publish.sh <sha> --dry-run --stop-after upstream
# full conversion and validation without uploading, then upload what was staged:
scripts/models/indic-flex/publish.sh <sha> --dry-run --run-prefix "nice -n 10"
scripts/models/indic-flex/publish.sh <sha>
```

Requirements:
- Apple Silicon macOS 15 or later, and `uv`.
- `hf auth login` with a write token, on an account that has accepted the upstream gate and can write to the
  `simple-voice` org.
- About 15 GB of free disk. Xcode is not needed: models are compiled with Core ML's own `MLModel.compileModel`.

How a run proceeds:
- Steps, in order: `env upstream extract reference trace coreml quantize contract compile validate stage publish`.
- Each heavy step runs in its own process and is skipped if its output already exists, so a rerun resumes.
- After a `--dry-run`, a plain run only uploads the folder that was staged.

## Flags

| Flag | Default | Meaning |
|---|---|---|
| `--dry-run` | off | everything except the upload; the staged repo is left in `WORK/repo` |
| `--work DIR` | `~/indic-flex-work/<sha12>` | work directory |
| `--stop-after STEP` | `publish` | stop after this step |
| `--run-prefix "CMD"` | none | prefix for the heavy steps, e.g. `nice -n 10` |
| `--keep` | off | keep the intermediates (by default each is deleted as soon as the next step has used it) |
| `--swap-growth-mb N` | 3072 | guard: abort a step if swap grows N MB over the pre-run baseline |
| `--swap-max-pct N` | 0 (off) | guard: abort if swap passes N% of the swap total |
| `--rss-max-gb N` | 12 | guard: abort if the step's RSS passes N GB |
| `--disk-floor-gb N` | 8 | guard: abort if free disk on the work volume falls below N GB |

When the guard aborts, the run exits with code 3 and the peaks are in `WORK/mem_peak.json`.

## Validation thresholds (a failure stops the run before staging)

The fixed clips in `clips/` (checked with `clips/SHA256SUMS`) are decoded twice:
- by NeMo in PyTorch (fp32, greedy, no KV cache);
- by the int4 Core ML models, using FluidAudio's own decode loop.

Hinglish clips use prompt (a), romanized Hindi `[7, 4, 18, 89, 89, 5, 9, 10, 13, 15]`. English clips use prompt
(d), `[7, 4, 18, 66, 66, 5, 9, 11, 13, 15]`.

The run fails unless:
- each clip's CER against the NeMo text is at most 0.10, and the mean is at most 0.05;
- the romanized (a) output is at least 95% Latin letters.

## Pins

| What | Pinned by |
|---|---|
| Python environment | `uv.lock` (Python 3.11, torch 2.7.0, coremltools 9.0, nemo_toolkit[asr] 2.7.3, huggingface_hub 0.36.2), `.python-version` |
| mobius | `588b8e7077d454e9613bdb2672b89824c2f7095d`, decode-algorithm reference only |
| Upstream | the revision sha passed on the command line |
| Licence | taken verbatim from that upstream revision; the card's `license_link` points at the repo copy |

First published run: upstream `ecba9bac9013c0340fe814b336897ee51ed80be9`, Hub commit
`2a7051a572d8ca1f4c9e86e82b693bc39d19ad35`, mean CER 0.0057.

After publishing a new upstream revision, pin the Hub commit it prints as `indicRevision` (and its
`downloadBytes` and upstream sha) in `engine/Sources/SimpleVoiceEngine/ModelCatalog.swift`.
