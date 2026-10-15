#!/usr/bin/env bash
# FilePath: scripts/eval/transcription-eval.sh
# Runs a folder of labelled clips through transcription models and reports word error rate,
# wrong-script output, speed and the quality numbers Smart Select gates on, so local models and
# each model's confidence floor are chosen from measurements. scripts/eval/make-clips.sh builds
# the English, Hindi and Hinglish clip sets.
#
# Clips: `<category>-<name>.wav` (16 kHz mono PCM16) with the expected text in `<same>.txt`.
# The category is the prefix before the first dash (`en`, `hi`, `hing`, ...). A clip without a
# `.txt` is still transcribed and reported, with an empty WER.
#
# Scoring, per clip:
# - WER: word edit distance over the reference length, case and punctuation ignored, Devanagari
#   nukta and chandrabindu folded (spelling variants, not recognition errors). Left empty when
#   the reference is not in the script the category expects (a Devanagari reference in `hing`).
# - sWER: the same over consonant skeletons that read alike in either script ("kal ki meeting"
#   and "कल की मीटिंग" both become "kl k mtng"), so romanised output is scored against any
#   reference without one spelling convention winning.
# - wrong script: `hi` expects Devanagari, so text without a Devanagari letter is wrong; every
#   other category expects no Devanagari letter at all.
#
# Usage: scripts/eval/transcription-eval.sh --clips DIR --models id[,id...] [--languages en,hi]
#            [--prompt TEXT] [--models-dir DIR] [--engine PATH] [--label NAME] [--out FILE]
# Model ids are engine model ids; everything runs on this Mac through the engine helper.
# `--label` names the single model in the output: a candidate's files placed under an existing
# model id in another --models-dir run through that id's decoding path under their own name.
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
label=""

while (($# > 0)); do
    case "$1" in
        --clips) clips="$2"; shift 2 ;;
        --models) models="$2"; shift 2 ;;
        --languages) languages="$2"; shift 2 ;;
        --prompt) prompt="$2"; shift 2 ;;
        --models-dir) models_dir="$2"; shift 2 ;;
        --engine) engine="$2"; shift 2 ;;
        --out) out="$2"; shift 2 ;;
        --label) label="$2"; shift 2 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done
if [[ -z "$clips" || -z "$models" ]]; then
    echo "usage: $0 --clips DIR --models id[,id...] [--languages en,hi] [--prompt TEXT]" >&2
    exit 2
fi
for tool in jq awk perl; do
    command -v "$tool" >/dev/null || { echo "missing required tool: $tool" >&2; exit 1; }
done
if [[ -n "$label" && "$models" == *,* ]]; then
    echo "--label names one model; run each candidate separately" >&2
    exit 2
fi

languages_json=$(jq -cn --arg l "$languages" '$l | split(",") | map(select(length > 0))')
now_ms() { perl -MTime::HiRes=time -e 'printf "%d\n", time() * 1000'; }

# Prints `errors words skeleton_errors skeleton_words` for a reference and a hypothesis.
read -r -d '' score_pl <<'PERL' || true
use strict; use warnings; use utf8; use Unicode::Normalize qw(NFD NFC);
my %dev = (
    'क' => 'k', 'ख' => 'k', 'ग' => 'g', 'घ' => 'g', 'ङ' => 'n', 'च' => 'c', 'छ' => 'c',
    'ज' => 'j', 'झ' => 'j', 'ञ' => 'n', 'ट' => 't', 'ठ' => 't', 'ड' => 'd', 'ढ' => 'd',
    'ण' => 'n', 'त' => 't', 'थ' => 't', 'द' => 'd', 'ध' => 'd', 'न' => 'n', 'प' => 'p',
    'फ' => 'p', 'ब' => 'b', 'भ' => 'b', 'म' => 'm', 'य' => 'y', 'र' => 'r', 'ल' => 'l',
    'व' => 'v', 'श' => 's', 'ष' => 's', 'स' => 's', 'ह' => 'h', 'ं' => 'n', 'ँ' => 'n',
);
sub words {
    my $s = NFD(lc shift);
    $s =~ s/\x{093C}//g;
    $s =~ s/\x{0901}/\x{0902}/g;
    $s =~ tr/०-९/0-9/;
    $s =~ s/[^\p{L}\p{M}\p{N}]+/ /g;
    return grep { length } map { NFC($_) } split / /, $s;
}
sub skeleton {
    my $w = NFD(shift);
    my $out = '';
    if ($w =~ /\p{Devanagari}/) {
        $out .= $dev{$_} // (/[0-9]/ ? $_ : '') for split //, $w;
    } else {
        $w =~ s/ck/k/g;
        $w =~ s/ch/C/g;
        $w =~ s/c(?=[eiy])/s/g;
        $w =~ s/c/k/g;
        $w =~ s/C/c/g;
        $w =~ s/(?<=[bcdfgjklmnpqrstvwxyz])h//g;
        $w =~ tr/fqwz/pkvj/;
        $w =~ s/x/ks/g;
        $w =~ s/[aeiou]//g;
        $out = $w;
    }
    $out =~ s/(.)\1+/$1/g;
    return $out;
}
sub distance {
    my ($r, $h) = @_;
    my @prev = (0 .. @$h);
    for my $i (1 .. @$r) {
        my @cur = ($i);
        for my $j (1 .. @$h) {
            my $sub = $prev[$j - 1] + ($r->[$i - 1] eq $h->[$j - 1] ? 0 : 1);
            my $best = $prev[$j] + 1 < $cur[$j - 1] + 1 ? $prev[$j] + 1 : $cur[$j - 1] + 1;
            push @cur, $sub < $best ? $sub : $best;
        }
        @prev = @cur;
    }
    return $prev[-1];
}
my @r = words($ARGV[0]); my @h = words($ARGV[1]);
my @rs = grep { length } map { skeleton($_) } @r;
my @hs = grep { length } map { skeleton($_) } @h;
printf "%d\t%d\t%d\t%d\n", distance(\@r, \@h), scalar @r, distance(\@rs, \@hs), scalar @rs;
PERL

has_devanagari() { ! perl -CSA -e 'exit($ARGV[0] =~ /\p{Devanagari}/ ? 1 : 0)' "$1"; }

row() {
    local model="$1" wav="$2" ms="$3" result="$4"
    local base category ref text errors="" words="" skel_errors="" skel_words="" wrong
    base=$(basename "$wav" .wav)
    category=${base%%-*}
    text=$(jq -r '.text // ""' <<<"$result")
    ref=""
    [[ -f "${wav%.wav}.txt" ]] && ref=$(<"${wav%.wav}.txt")
    if [[ -n "$ref" ]]; then
        IFS=$'\t' read -r errors words skel_errors skel_words \
            < <(perl -CSA -e "$score_pl" "$ref" "$text")
        if [[ "$category" != "hi" ]] && has_devanagari "$ref"; then
            errors=""; words=""
        fi
    fi
    if [[ "$category" == "hi" ]]; then
        if has_devanagari "$text"; then wrong=0; else wrong=1; fi
    else
        if has_devanagari "$text"; then wrong=1; else wrong=0; fi
    fi
    jq -rn --arg model "${label:-$model}" --arg category "$category" --arg clip "$base" \
        --arg ms "$ms" --arg errors "$errors" --arg words "$words" \
        --arg skel_errors "$skel_errors" --arg skel_words "$skel_words" --arg wrong "$wrong" \
        --arg text "$text" --argjson r "$result" \
        '[$model, $category, $clip, $ms, $errors, $words, $skel_errors, $skel_words, $wrong,
          ($r.language // ""), ($r.quality.avgLogprob // ""), ($r.quality.compressionRatio // ""),
          ($r.quality.noSpeechProb // ""), ($r.quality.confidence // ""), $text] | @tsv'
}

shopt -s nullglob
wavs=("$clips"/*.wav)
((${#wavs[@]} > 0)) || { echo "no .wav clips in $clips" >&2; exit 1; }

tsv=$(mktemp)
trap 'rm -f "$tsv"' EXIT
printf 'model\tcategory\tclip\tms\terrors\twords\tskeleton_errors\tskeleton_words\twrong_script\tlanguage\tavg_logprob\tcompression_ratio\tno_speech_prob\tconfidence\ttext\n' >"$tsv"

IFS=',' read -r -a model_list <<<"$models"
for model in "${model_list[@]}"; do
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
        k = $1 "\t" $2; n[k]++; ms[k] += $4; wrong[k] += $9
        if ($6 != "") { err[k] += $5; words[k] += $6 }
        if ($8 != "") { serr[k] += $7; swords[k] += $8 }
        if ($11 != "") { lp[k] += $11; lpn[k]++ }
        if ($14 != "") { cf[k] += $14; cfn[k]++ }
    }
    END {
        printf "%-24s %-6s %5s %7s %7s %7s %8s %10s %10s\n", "model", "cat", "clips", "WER%",
            "sWER%", "wrong%", "avg ms", "logprob", "confidence"
        for (k in n) {
            split(k, p, "\t")
            printf "%-24s %-6s %5d %7s %7s %6.0f%% %8.0f %10s %10s\n", p[1], p[2], n[k],
                (words[k] ? sprintf("%.1f", 100 * err[k] / words[k]) : "-"),
                (swords[k] ? sprintf("%.1f", 100 * serr[k] / swords[k]) : "-"),
                100 * wrong[k] / n[k], ms[k] / n[k],
                (lpn[k] ? sprintf("%.3f", lp[k] / lpn[k]) : "-"),
                (cfn[k] ? sprintf("%.3f", cf[k] / cfn[k]) : "-")
        }
    }' "$tsv" | sort >&2
