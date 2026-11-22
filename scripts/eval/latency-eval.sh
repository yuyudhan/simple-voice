#!/usr/bin/env bash
# FilePath: scripts/eval/latency-eval.sh
# Measures what formatting while speaking (requirements LT-5) saves after the stop: for each clip,
# today's stop (transcribe the whole recording, format, whole-text pass) against the speculative
# one (the clip replayed in real time while the app's worker drafts the recording so far and
# formats closed segments of it, then reuse plus the tail), both timed from the end of the audio
# to the text that would be pasted.
# The work is done by crates/sv-text/examples/latency, through the engine helper, with the app's
# own `sv_text::speculation` code; see that file for the method and the TSV columns.
#
# Usage: scripts/eval/latency-eval.sh --clips DIR --models id[,id...] --formatters id[,id...]
#            [--compose] [--dictionary FILE] [--models-dir DIR] [--engine PATH]
#            [--style formal|casual] [--languages en] [--out-dir DIR]
# `--compose` benches the clips of DIR as they are plus composites of consecutive clips (~30, ~88,
# ~183 and ~336 words), since short clips leave nothing to format while speaking. `--dictionary`
# is a personal dictionary file as transcription-eval.sh takes it. Each model and formatter pair
# writes `latency-<model>-<formatter>.tsv` to --out-dir (default: the current directory) and prints
# its per-bucket summary (median and p90 stop→text, reuse, words moved, edits against the
# rules-only text) on stderr. Real-time
# streaming makes a run as long as the clips' audio, plus the stops.
set -euo pipefail

clips=""
models=""
formatters=""
compose=0
dictionary=""
# The debug engine pairs with the development data directory, where `just dev` downloads models.
models_dir="${HOME}/.simplevoice-dev/models"
engine="engine/.build/debug/simple-voice-engine"
style="formal"
languages="en"
out_dir="."

while (($# > 0)); do
    case "$1" in
    --clips | --models | --formatters | --dictionary | --models-dir | --engine | --style | --languages | --out-dir)
        [[ $# -ge 2 ]] || {
            echo "$1 needs a value" >&2
            exit 2
        }
        name=${1#--}
        name=${name//-/_}
        printf -v "$name" '%s' "$2"
        shift 2
        ;;
    --compose)
        compose=1
        shift
        ;;
    *)
        echo "unknown argument: $1" >&2
        exit 2
        ;;
    esac
done
if [[ -z "$clips" || -z "$models" || -z "$formatters" ]]; then
    echo "usage: $0 --clips DIR --models id[,id...] --formatters id[,id...] [--compose]" >&2
    exit 2
fi
[[ -x "$engine" ]] || {
    echo "engine helper not built: $engine (run: just engine)" >&2
    exit 1
}

cargo build --quiet -p sv-text --example latency
bench="$(cargo metadata --format-version 1 --no-deps | jq -r .target_directory)/debug/examples/latency"

if ((compose)); then
    composed=$(mktemp -d)
    trap 'rm -rf "$composed"' EXIT
    "$bench" compose --clips "$clips" --out "$composed"
    clips=$composed
fi

mkdir -p "$out_dir"
IFS=',' read -r -a model_list <<<"$models"
IFS=',' read -r -a formatter_list <<<"$formatters"
for model in "${model_list[@]}"; do
    for formatter in "${formatter_list[@]}"; do
        args=(run --clips "$clips" --engine "$engine" --models-dir "$models_dir" --asr "$model"
            --formatter "$formatter" --style "$style" --languages "$languages"
            --out "$out_dir/latency-$model-$formatter.tsv")
        [[ -n "$dictionary" ]] && args+=(--dictionary "$dictionary")
        echo "== $model + $formatter" >&2
        "$bench" "${args[@]}"
    done
done
