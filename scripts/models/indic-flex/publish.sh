#!/usr/bin/env bash
# FilePath: scripts/models/indic-flex/publish.sh
# Converts bodhan-ai/indic-transcribe-flex at one upstream revision to FluidAudio 0.17.4's Canary Core ML int4
# layout, validates the int4 decode against NeMo, and publishes it to Hugging Face
# (simple-voice/indic-transcribe-flex-coreml, public and ungated, tagged flex-<sha12>).
#
# Usage: scripts/models/indic-flex/publish.sh <upstream-flex-revision-sha> [options]
#   --dry-run              do everything except the upload (stage folder + manifest are left in WORK/repo)
#   --work DIR             work directory (default: ~/indic-flex-work/<sha12>); needs ~15 GB free
#   --stop-after STEP      stop after STEP (env upstream extract reference trace coreml quantize contract
#                          compile validate stage publish); e.g. `--dry-run --stop-after upstream` runs only the
#                          checks that need no NeMo load
#   --run-prefix "CMD"     prefix for the heavy steps (e.g. "nice -n 10")
#   --keep                 keep every intermediate (.pt, .mlpackage, the .nemo and its extraction); by default each
#                          is deleted as soon as the next step has consumed it
#   --swap-growth-mb N     guard: abort a heavy step if swap grows N MB over the pre-run baseline (3072)
#   --swap-max-pct N       guard: abort if swap passes N% of the swap total (default off: macOS grows swap)
#   --rss-max-gb N         guard: abort if the step's RSS passes N GB (12)
#   --disk-floor-gb N      guard: abort if free disk on the work volume falls below N GB (8)
#
# Needs: Apple Silicon macOS 15+, uv, and a Hugging Face
# login with access to the gated upstream repo (`hf auth login` in WORK/venv) and write access to the
# simple-voice org for the upload. The Python environment is pinned by uv.lock in this directory; mobius is
# pinned below. Each heavy step runs in its own process and is skipped when its output already exists, so a
# rerun resumes. A memory-guard abort exits 3 and leaves WORK/mem_peak.json with the measured peaks.
#
# Validation (fails the run): the fixed clips in clips/ (macOS `say`, Lekha for Hinglish written in Devanagari,
# Rishi/Samantha for English; committed as files because `say` output changes between macOS releases) are
# decoded by NeMo (PyTorch) and by the int4 Core ML models with FluidAudio's algorithm, Hinglish with the
# romanized Hindi prompt and English with the English prompt. Each clip's CER against NeMo must be <= 0.10,
# the mean <= 0.05, and romanized output must be >= 95% Latin letters.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ID="simple-voice/indic-transcribe-flex-coreml"
UPSTREAM="bodhan-ai/indic-transcribe-flex"
MOBIUS_URL="https://github.com/FluidInference/mobius.git"
MOBIUS_COMMIT="588b8e7077d454e9613bdb2672b89824c2f7095d"
STEPS=(env upstream extract reference trace coreml quantize contract compile validate stage publish)
PARTS=(Preprocessor Encoder Decoder Projection)

usage() { sed -n '2,31p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; }

rev=""
dry_run=0
work=""
stop_after="publish"
run_prefix=""
keep=0
guard_args=()
while [ $# -gt 0 ]; do
    case "$1" in
        -h | --help) usage; exit 0 ;;
        --dry-run) dry_run=1 ;;
        --work) work="$2"; shift ;;
        --stop-after) stop_after="$2"; shift ;;
        --run-prefix) run_prefix="$2"; shift ;;
        --keep) keep=1; guard_args+=("--keep") ;;
        --swap-growth-mb | --swap-max-pct | --rss-max-gb | --disk-floor-gb) guard_args+=("$1" "$2"); shift ;;
        -*) echo "publish.sh: unknown option $1" >&2; usage >&2; exit 2 ;;
        *)
            [ -z "$rev" ] || { echo "publish.sh: one revision only" >&2; exit 2; }
            rev="$1"
            ;;
    esac
    shift
done
if ! [[ "$rev" =~ ^[0-9a-f]{40}$ ]]; then
    echo "publish.sh: need the full 40-hex upstream revision sha of ${UPSTREAM} (got '${rev}')" >&2
    usage >&2
    exit 2
fi
known=0
for s in "${STEPS[@]}"; do [ "$s" = "$stop_after" ] && known=1; done
[ "$known" = 1 ] || { echo "publish.sh: unknown step '${stop_after}'" >&2; exit 2; }
if [ "$dry_run" = 1 ] && [ "$stop_after" = "publish" ]; then stop_after="stage"; fi

sha12="${rev:0:12}"
work="${work:-$HOME/indic-flex-work/$sha12}"
mkdir -p "$work"
work="$(cd "$work" && pwd)"
py="$work/venv/bin/python"
read -r -a prefix <<<"$run_prefix"

log() { printf '%s publish.sh: %s\n' "$(date +%H:%M:%S)" "$*" >&2; }
done_step() {
    log "step $1 done"
    if [ "$1" = "$stop_after" ]; then
        log "stopping after $1 (work: $work)"
        exit 0
    fi
}
flex() { "$py" "$HERE/indic_flex.py" "$@" --work "$work" --revision "$rev" "${guard_args[@]+"${guard_args[@]}"}"; }
heavy() { "${prefix[@]+"${prefix[@]}"}" "$py" "$HERE/indic_flex.py" "$@" --work "$work" --revision "$rev" \
    "${guard_args[@]+"${guard_args[@]}"}"; }
# A part is done once its Core ML package (fp32/fp16 or int4) or its compiled model exists.
part_done() {
    local p="$1"
    [ -d "$work/$p.mlpackage" ] && [ "$p" != Encoder ] && [ "$p" != Decoder ] && return 0
    [ -d "$work/${p}Int4.mlpackage" ] || [ -d "$work/compiled/$p.mlmodelc" ] || [ -d "$work/compiled/${p}Int4.mlmodelc" ]
}
# NeMo (and so the extracted .nemo) is needed until the reference and every part are done.
need_nemo() {
    [ -f "$work/reference.json" ] || return 0
    local p
    for p in "${PARTS[@]}"; do part_done "$p" || return 0; done
    return 1
}
compiled_all() {
    local m
    for m in Preprocessor EncoderInt4 DecoderInt4 Projection; do [ -d "$work/compiled/$m.mlmodelc" ] || return 1; done
}

# ---- env: pinned uv environment, tools, login, pinned mobius
for tool in uv git curl shasum tar; do
    command -v "$tool" >/dev/null || { echo "publish.sh: $tool not found" >&2; exit 1; }
done
[ "$(uname -s)/$(uname -m)" = "Darwin/arm64" ] || { echo "publish.sh: needs Apple Silicon macOS" >&2; exit 1; }
log "uv sync (frozen) -> $work/venv"
UV_PROJECT_ENVIRONMENT="$work/venv" uv sync --frozen --project "$HERE" --quiet
if ! "$work/venv/bin/hf" auth whoami >/dev/null 2>&1; then
    echo "publish.sh: not logged in to Hugging Face; run '$work/venv/bin/hf auth login' (write token)" >&2
    exit 1
fi
if [ ! -d "$work/mobius/.git" ]; then
    git init -q "$work/mobius"
    git -C "$work/mobius" sparse-checkout set models/stt/canary-1b-v2/coreml
    git -C "$work/mobius" fetch -q --depth 1 --filter=blob:none "$MOBIUS_URL" "$MOBIUS_COMMIT"
    git -C "$work/mobius" checkout -q FETCH_HEAD
fi
[ "$(git -C "$work/mobius" rev-parse HEAD)" = "$MOBIUS_COMMIT" ] || { echo "publish.sh: mobius not at pin" >&2; exit 1; }
test -f "$work/mobius/models/stt/canary-1b-v2/coreml/torch_reference.py"
(cd "$HERE/clips" && shasum -a 256 -c --quiet SHA256SUMS)
done_step env

# A finished stage (validated, manifest written) survives the cleanup below; a later run without --dry-run
# then only publishes it. Delete WORK/manifest.json to rebuild from scratch.
staged=0
if [ -f "$work/manifest.json" ] && [ -f "$work/repo/README.md" ]; then
    staged=1
    log "found a validated stage in $work/repo; skipping to publish"
fi
# The memory guard's baseline is taken by the first heavy step of this run (before its NeMo load).
rm -f "$work/swap_baseline.json"

# ---- upstream: download the pinned revision; tokenizer, prompt and licence checks; vocab.json
if [ "$staged" = 0 ]; then
    log "hf download ${UPSTREAM}@${rev}"
    excludes=("model.safetensors" "*.png")
    # Once extracted (or no longer needed), the 4.6 GB .nemo is not fetched again.
    if [ -f "$work/nemo/model_weights.ckpt" ] || ! need_nemo; then excludes+=("nemo/*.nemo"); fi
    "$work/venv/bin/hf" download "$UPSTREAM" --revision "$rev" --local-dir "$work/flex" \
        --exclude "${excludes[@]}" >/dev/null
    flex upstream
fi
done_step upstream

# ---- extract: unpack the .nemo once so NeMo restores from a directory with memory-mapped weights
if [ "$staged" = 0 ] && [ ! -f "$work/nemo/model_weights.ckpt" ] && need_nemo; then
    rm -rf "${work:?}/nemo.partial"
    mkdir -p "$work/nemo.partial"
    tar -xf "$work"/flex/nemo/*.nemo -C "$work/nemo.partial"
    mv "$work/nemo.partial" "$work/nemo"
    [ "$keep" = 1 ] || rm -f "$work"/flex/nemo/*.nemo
fi
done_step extract

# ---- reference: NeMo PyTorch greedy decode of the fixed clips
if [ "$staged" = 0 ] && [ ! -f "$work/reference.json" ]; then
    heavy reference
fi
done_step reference

# ---- trace -> coreml (-> quantize) per part: one NeMo load per part. Each intermediate is deleted as soon as the
# next step has consumed it: the .pt once its .mlpackage exists, the fp16 encoder/decoder package once its int4
# package exists, and (in compile) each .mlpackage once its .mlmodelc is verified.
if [ "$staged" = 0 ]; then
    for part in "${PARTS[@]}"; do
        part_done "$part" && continue
        [ -f "$work/$part.pt" ] || [ -d "$work/$part.mlpackage" ] || heavy trace --part "$part"
        [ -d "$work/$part.mlpackage" ] || heavy coreml --part "$part"
        [ "$keep" = 1 ] || rm -f "$work/$part.pt"
        if [ "$part" = Encoder ] || [ "$part" = Decoder ]; then
            heavy quantize --part "$part"
            [ "$keep" = 1 ] || rm -rf "${work:?}/$part.mlpackage"
        fi
    done
    # Every NeMo consumer is done: the extracted checkpoint goes too.
    need_nemo || [ "$keep" = 1 ] || rm -rf "${work:?}/nemo"
fi
done_step trace
done_step coreml
done_step quantize

[ "$staged" = 1 ] || compiled_all || flex contract
done_step contract

# ---- compile: .mlpackage -> .mlmodelc (what FluidAudio loads), with Core ML's own compiler; checks int4 weights
[ "$staged" = 1 ] || heavy compile
done_step compile

[ "$staged" = 1 ] || heavy validate
done_step validate

if [ "$staged" = 0 ]; then
    # Provenance for the card: the simple-voice commit holding this script, or a plain note when it isn't one.
    if git -C "$HERE" ls-files --error-unmatch publish.sh >/dev/null 2>&1; then
        sv_ref="commit $(git -C "$HERE" rev-parse --short=12 HEAD)"
        git -C "$HERE" diff --quiet HEAD -- . || sv_ref="${sv_ref} with local changes"
    else
        sv_ref="run before the script was committed there"
    fi
    flex stage --mobius-commit "$MOBIUS_COMMIT" --simple-voice-ref "$sv_ref"
    if [ "$keep" = 0 ]; then
        rm -rf "${work:?}/nemo" "$work"/flex/nemo/*.nemo "$work"/*.mlpackage
    fi
fi
"$py" -c 'import json,sys; m=json.load(open(sys.argv[1])); [print(v["sha256"], v["bytes"], k) for k,v in m.items()]; print("total", sum(v["bytes"] for v in m.values()))' "$work/manifest.json"
done_step stage

# ---- publish: public, ungated, tag flex-<sha12>, anonymous verification
flex publish --repo "$REPO_ID"
status="$(env -u HF_TOKEN curl -sI "https://huggingface.co/${REPO_ID}/resolve/main/vocab.json" | awk 'NR==1' | tr -d '\r')"
echo "anonymous curl -fsI vocab.json: ${status}" | tee "$work/anon_curl.txt"
env -u HF_TOKEN curl -fsI -o /dev/null "https://huggingface.co/${REPO_ID}/resolve/main/vocab.json" ||
    { echo "publish.sh: anonymous download of vocab.json failed" >&2; exit 1; }
done_step publish
