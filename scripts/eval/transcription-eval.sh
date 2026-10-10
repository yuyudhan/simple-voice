#!/usr/bin/env bash
# FilePath: scripts/eval/transcription-eval.sh
# Phase 0 of Smart Select: runs a folder of labelled clips through transcription models and
# reports word error rate, romanisation, speed and the quality numbers Smart Select gates on,
# so the local Hinglish model and each model's confidence floor are chosen from measurements.
#
# Clips: `<category>-<name>.wav` (16 kHz mono PCM16) with the expected text in `<same>.txt`.
# The category is the prefix before the first dash (`en`, `hi`, `hing`, ...). A clip without a
# `.txt` is still transcribed and reported, with an empty WER.
#
# Usage: scripts/eval/transcription-eval.sh --clips DIR --models id[,id...] [--languages en,hi]
#            [--prompt TEXT] [--models-dir DIR] [--engine PATH] [--out FILE]
# Model ids are engine model ids, plus `groq-whisper` (needs GROQ_API_KEY in the environment).
# Output: one TSV row per clip and model (to --out, default stdout), then a per-model,
# per-category summary on stderr.
set -euo pipefail

clips=""
models=""
languages="en,hi"
prompt="Haan, toh main ab yeh code check karta hoon."
# The debug engine pairs with the development data directory, where `just dev` downloads models.
models_dir="${HOME}/.simplevoice-dev/models"
engine="engine/.build/debug/simple-voice-engine"
out="/dev/stdout"

while (($# > 0)); do
    case "$1" in
        --clips) clips="$2"; shift 2 ;;
        --models) models="$2"; shift 2 ;;
        --languages) languages="$2"; shift 2 ;;
        --prompt) prompt="$2"; shift 2 ;;
        --models-dir) models_dir="$2"; shift 2 ;;
        --engine) engine="$2"; shift 2 ;;
        --out) out="$2"; shift 2 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done
if [[ -z "$clips" || -z "$models" ]]; then
    echo "usage: $0 --clips DIR --models id[,id...] [--languages en,hi] [--prompt TEXT]" >&2
    exit 2
fi
for tool in jq awk curl; do
    command -v "$tool" >/dev/null || { echo "missing required tool: $tool" >&2; exit 1; }
done

languages_json=$(jq -cn --arg l "$languages" '$l | split(",") | map(select(length > 0))')
now_ms() { perl -MTime::HiRes=time -e 'printf "%d\n", time() * 1000'; }

# Lowercases, strips punctuation (ASCII and the Devanagari danda) and splits into words, then
# prints the word-level edit distance and the reference word count.
word_errors() {
    awk -v ref="$1" -v hyp="$2" '
        function norm(s) {
            s = tolower(s)
            gsub(/[.,!?;:"()\[\]'"'"'`।॥-]/, " ", s)
            return s
        }
        BEGIN {
            n = split(norm(ref), r, /[ \t\n]+/); m = split(norm(hyp), h, /[ \t\n]+/)
            rn = 0; for (i = 1; i <= n; i++) if (r[i] != "") R[++rn] = r[i]
            hn = 0; for (j = 1; j <= m; j++) if (h[j] != "") H[++hn] = h[j]
            for (i = 0; i <= rn; i++) d[i, 0] = i
            for (j = 0; j <= hn; j++) d[0, j] = j
            for (i = 1; i <= rn; i++) for (j = 1; j <= hn; j++) {
                c = (R[i] == H[j]) ? 0 : 1
                v = d[i-1, j] + 1; if (d[i, j-1] + 1 < v) v = d[i, j-1] + 1
                if (d[i-1, j-1] + c < v) v = d[i-1, j-1] + c
                d[i, j] = v
            }
            printf "%d\t%d\n", d[rn, hn], rn
        }'
}

# Aggregates Whisper segments exactly as Smart Select does: token-weighted means of
# avg_logprob and no_speech_prob, the maximum compression ratio.
read -r -d '' groq_quality <<'JQ' || true
(.segments // []) as $s
    | if ($s | length) == 0 then {} else
        ($s | map((.tokens // []) | length | if . == 0 then 1 else . end)) as $w
        | ($w | add) as $total
        | {avgLogprob: ([range($s | length)] | map($s[.].avg_logprob * $w[.]) | add / $total),
           noSpeechProb: ([range($s | length)] | map($s[.].no_speech_prob * $w[.]) | add / $total),
           compressionRatio: ($s | map(.compression_ratio) | max)}
      end
JQ

transcribe_groq() {
    local wav="$1" body code attempt
    : "${GROQ_API_KEY:?groq-whisper needs GROQ_API_KEY}"
    body=$(mktemp)
    # The free tier allows about 20 requests a minute; wait out a rate limit instead of failing.
    for attempt in 1 2 3 4 5 6; do
        code=$(curl -sS -o "$body" -w '%{http_code}' \
            https://api.groq.com/openai/v1/audio/transcriptions \
            -H "Authorization: Bearer ${GROQ_API_KEY}" \
            -F model=whisper-large-v3-turbo -F response_format=verbose_json \
            -F "prompt=${prompt}" -F "file=@${wav}")
        [[ "$code" != "429" ]] && break
        sleep $((attempt * 10))
    done
    if [[ "$code" != "200" ]]; then
        echo "groq-whisper $(basename "$wav"): HTTP $code $(<"$body")" >&2
        rm -f "$body"
        return 1
    fi
    jq -c "{text: (.text // \"\" | gsub(\"^\\\\s+|\\\\s+$\"; \"\")),
            language: .language, quality: (${groq_quality})}" "$body"
    rm -f "$body"
}

row() {
    local model="$1" wav="$2" ms="$3" result="$4"
    local base category ref text errors words romanised
    base=$(basename "$wav" .wav)
    category=${base%%-*}
    text=$(jq -r '.text // ""' <<<"$result")
    ref=""
    [[ -f "${wav%.wav}.txt" ]] && ref=$(<"${wav%.wav}.txt")
    errors=""; words=""
    if [[ -n "$ref" ]]; then
        IFS=$'\t' read -r errors words < <(word_errors "$ref" "$text")
    fi
    # Romanised = no Devanagari code point in the output.
    if perl -CS -ne 'exit(/\p{Devanagari}/ ? 1 : 0)' <<<"$text"; then romanised=1; else romanised=0; fi
    jq -rn --arg model "$model" --arg category "$category" --arg clip "$base" --arg ms "$ms" \
        --arg errors "$errors" --arg words "$words" --arg romanised "$romanised" \
        --arg text "$text" --argjson r "$result" \
        '[$model, $category, $clip, $ms, $errors, $words, $romanised, ($r.language // ""),
          ($r.quality.avgLogprob // ""), ($r.quality.compressionRatio // ""),
          ($r.quality.noSpeechProb // ""), ($r.quality.confidence // ""), $text] | @tsv'
}

shopt -s nullglob
wavs=("$clips"/*.wav)
((${#wavs[@]} > 0)) || { echo "no .wav clips in $clips" >&2; exit 1; }

tsv=$(mktemp)
trap 'rm -f "$tsv"' EXIT
printf 'model\tcategory\tclip\tms\terrors\twords\tromanised\tlanguage\tavg_logprob\tcompression_ratio\tno_speech_prob\tconfidence\ttext\n' >"$tsv"

IFS=',' read -r -a model_list <<<"$models"
for model in "${model_list[@]}"; do
    if [[ "$model" == "groq-whisper" ]]; then
        for wav in "${wavs[@]}"; do
            start=$(now_ms)
            result=$(transcribe_groq "$wav")
            row "$model" "$wav" $(($(now_ms) - start)) "$result" >>"$tsv"
        done
        continue
    fi
    [[ -x "$engine" ]] || { echo "engine helper not built: $engine (run: just engine)" >&2; exit 1; }
    coproc ENGINE { "$engine" --models-dir "$models_dir" 2>/dev/null; }
    id=0
    # Load first, so the per-clip times measure inference only.
    printf '%s\n' "$(jq -cn --arg m "$model" '{id: 0, cmd: "preload", model: $m}')" >&"${ENGINE[1]}"
    while IFS= read -r line <&"${ENGINE[0]}"; do
        [[ $(jq -r '.id // empty' <<<"$line") == "0" && $(jq -r 'has("ok")' <<<"$line") == "true" ]] && break
    done
    for wav in "${wavs[@]}"; do
        id=$((id + 1))
        request=$(jq -cn --argjson id "$id" --arg m "$model" --arg w "$(cd "$(dirname "$wav")" && pwd)/$(basename "$wav")" \
            --argjson l "$languages_json" --arg p "$prompt" \
            '{id: $id, cmd: "transcribe", model: $m, wavPath: $w, languages: $l, prompt: $p}')
        start=$(now_ms)
        printf '%s\n' "$request" >&"${ENGINE[1]}"
        while IFS= read -r line <&"${ENGINE[0]}"; do
            [[ $(jq -r '.id // empty' <<<"$line") == "$id" && $(jq -r 'has("ok")' <<<"$line") == "true" ]] && break
        done
        ms=$(($(now_ms) - start))
        if [[ $(jq -r '.ok' <<<"$line") == "true" ]]; then
            row "$model" "$wav" "$ms" "$(jq -c '.result' <<<"$line")" >>"$tsv"
        else
            echo "$model $(basename "$wav"): $(jq -r '.error' <<<"$line")" >&2
        fi
    done
    engine_in=${ENGINE[1]}
    exec {engine_in}>&-
    wait "$ENGINE_PID" || true
done

cat "$tsv" >"$out"
awk -F'\t' 'NR > 1 {
        k = $1 "\t" $2; n[k]++; ms[k] += $4; rom[k] += $7
        if ($6 != "") { err[k] += $5; words[k] += $6 }
        if ($9 != "") { lp[k] += $9; lpn[k]++ }
        if ($12 != "") { cf[k] += $12; cfn[k]++ }
    }
    END {
        printf "%-24s %-6s %5s %7s %8s %9s %10s %10s\n", "model", "cat", "clips", "WER%", "avg ms",
            "romanised", "logprob", "confidence"
        for (k in n) {
            split(k, p, "\t")
            printf "%-24s %-6s %5d %7s %8.0f %8.0f%% %10s %10s\n", p[1], p[2], n[k],
                (words[k] ? sprintf("%.1f", 100 * err[k] / words[k]) : "-"), ms[k] / n[k],
                100 * rom[k] / n[k], (lpn[k] ? sprintf("%.3f", lp[k] / lpn[k]) : "-"),
                (cfn[k] ? sprintf("%.3f", cf[k] / cfn[k]) : "-")
        }
    }' "$tsv" | sort >&2
