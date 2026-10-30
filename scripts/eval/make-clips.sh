#!/usr/bin/env bash
# FilePath: scripts/eval/make-clips.sh
# Builds the English, Hindi, Hinglish and names clip sets transcription-eval.sh reads, one folder
# per dictation language, so each set runs with the languages and prompt the app sends for it:
#
#   <out>/en/     en-<id>-<voice>      English, spoken by Rishi and Lekha (both Indian accents)
#   <out>/hi/     hi-<id>-<voice>      Hindi mode: reference in Devanagari, English words in Roman
#   <out>/hing/   hing-<id>-<voice>    Hinglish mode: the same audio, reference in Roman script
#   <out>/names/  names-<id>-<voice>   English with names, for `--dictionary
#                                      scripts/eval/names-dictionary.txt`
#
# Speech comes from macOS `say` (scripts/eval/say-corpus.tsv): Lekha reads the Devanagari text,
# Rishi reads the romanised text, so every Hindi sentence is heard in two voices. `--fleurs N`
# adds the first N distinct sentences of the Hindi test split of Google's FLEURS (CC BY 4.0,
# real speakers) at a pinned revision; their Devanagari reference serves both folders, and the
# Hinglish set scores them by script-independent skeleton WER only.
#
# Usage: scripts/eval/make-clips.sh --out DIR [--fleurs N] [--cache DIR]
set -euo pipefail

out=""
fleurs=0
cache="${TMPDIR:-/tmp}/sv-eval-cache"
corpus="$(cd "$(dirname "$0")" && pwd)/say-corpus.tsv"
fleurs_revision="70bb2e84b976b7e960aa89f1c648e09c59f894dd"
fleurs_tar_sha256="47c238b9cf7016ba596639bb3038b0c89dc6e85759f16d09c4f58c6dfa25c65f"
fleurs_tsv_sha256="889e82e2875490f9533ff59ab67326fcfa01cf828fc4412758aaaa1f442a729f"

while (($# > 0)); do
    case "$1" in
        --out) out="$2"; shift 2 ;;
        --fleurs) fleurs="$2"; shift 2 ;;
        --cache) cache="$2"; shift 2 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done
[[ -n "$out" ]] || { echo "usage: $0 --out DIR [--fleurs N] [--cache DIR]" >&2; exit 2; }
for tool in say afconvert curl shasum tar; do
    command -v "$tool" >/dev/null || { echo "missing required tool: $tool" >&2; exit 1; }
done
mkdir -p "$out/en" "$out/hi" "$out/hing" "$out/names"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# 16 kHz mono PCM16, the format the app records.
to_wav() { afconvert -f WAVE -d LEI16@16000 -c 1 "$1" "$2"; }

speak() {
    local voice="$1" text="$2" wav="$3"
    say -v "$voice" -o "$work/say.aiff" -- "$text"
    to_wav "$work/say.aiff" "$wav"
}

# The neural Indian English voices (Aman, Tara) abort in `say` on some macOS builds; Rishi and
# Lekha (the Hindi voice, reading English) are the Indian-accented voices that always run.
while IFS=$'\t' read -r id native romanised; do
    [[ -z "$id" || "$id" == \#* ]] && continue
    case "$id" in
        en-* | names-*)
            folder=${id%%-*}
            for voice in Rishi Lekha; do
                name="${id}-$(tr '[:upper:]' '[:lower:]' <<<"$voice")"
                speak "$voice" "$native" "$out/$folder/$name.wav"
                printf '%s\n' "$native" >"$out/$folder/$name.txt"
            done
            ;;
        *)
            for voice in Lekha Rishi; do
                text=$native
                [[ "$voice" == Rishi ]] && text=$romanised
                name="${id}-$(tr '[:upper:]' '[:lower:]' <<<"$voice")"
                speak "$voice" "$text" "$out/hi/hi-$name.wav"
                cp "$out/hi/hi-$name.wav" "$out/hing/hing-$name.wav"
                printf '%s\n' "$native" >"$out/hi/hi-$name.txt"
                printf '%s\n' "$romanised" >"$out/hing/hing-$name.txt"
            done
            ;;
    esac
done <"$corpus"

if ((fleurs > 0)); then
    base="https://huggingface.co/datasets/google/fleurs/resolve/${fleurs_revision}/data/hi_in"
    mkdir -p "$cache"
    fetch() {
        local url="$1" file="$2" sum="$3"
        if [[ ! -f "$file" ]] || ! shasum -a 256 -c <<<"$sum  $file" >/dev/null 2>&1; then
            curl -fsSL -o "$file.part" "$url"
            mv "$file.part" "$file"
        fi
        shasum -a 256 -c <<<"$sum  $file" >/dev/null || { echo "checksum mismatch: $file" >&2; exit 1; }
    }
    fetch "$base/test.tsv" "$cache/fleurs-hi-test.tsv" "$fleurs_tsv_sha256"
    fetch "$base/audio/test.tar.gz" "$cache/fleurs-hi-test.tar.gz" "$fleurs_tar_sha256"
    # Columns: sentence id, file, raw transcription, ...; one recording per sentence.
    awk -F'\t' -v n="$fleurs" '!seen[$1]++ { if (++taken > n) exit; print $2 "\t" $3 }' \
        "$cache/fleurs-hi-test.tsv" >"$work/fleurs.tsv"
    cut -f1 "$work/fleurs.tsv" | sed 's|^|test/|' >"$work/members"
    tar -xzf "$cache/fleurs-hi-test.tar.gz" -C "$work" -T "$work/members"
    k=0
    while IFS=$'\t' read -r file text; do
        k=$((k + 1))
        name=$(printf 'fleurs-%02d' "$k")
        to_wav "$work/test/$file" "$out/hi/hi-$name.wav"
        cp "$out/hi/hi-$name.wav" "$out/hing/hing-$name.wav"
        printf '%s\n' "$text" >"$out/hi/hi-$name.txt"
        printf '%s\n' "$text" >"$out/hing/hing-$name.txt"
    done <"$work/fleurs.tsv"
fi

for set in en hi hing names; do
    echo "$set: $(find "$out/$set" -name '*.wav' | wc -l | tr -d ' ') clips" >&2
done
