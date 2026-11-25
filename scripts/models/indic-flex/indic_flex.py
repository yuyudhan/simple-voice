#!/usr/bin/env python3
"""Indic-Transcribe-Flex (.nemo) -> FluidAudio 0.17.4 Canary Core ML int4, staged for Hugging Face.

Driven by publish.sh; every subcommand reads and writes under --work (upstream download in WORK/flex,
pre-extracted .nemo in WORK/nemo). Each NeMo load runs in its own process, one model part at a time.

  upstream   tokenizer/layout/prompt/licence checks on the downloaded Flex revision; writes vocab.json
  reference  NeMo load: PyTorch greedy decode of the fixed clips (fp32, no KV cache) -> reference.json
  trace      NeMo load, keep one part (--part), drop the rest, torch.jit.trace -> WORK/<Part>.pt
  coreml     traced part -> .mlpackage (no NeMo in memory)
  quantize   Encoder / Decoder -> <Part>Int4: int4 linear_symmetric, per_block 32 (iOS 18 / macOS 15)
  contract   input/output names, dtypes and shapes of the four packages against FluidAudio's contract
  validate   int4 Core ML greedy decode (FluidAudio's algorithm) vs the NeMo reference; fails on threshold
  stage      assembles the repo folder: models, vocab.json, metadata.json, README.md, licence, NOTICE.md
  publish    uploads the staged folder (public, ungated), tags flex-<sha12>, verifies every file anonymously
"""
from __future__ import annotations

import argparse
import gc
import hashlib
import json
import os
import re
import subprocess
import sys
import threading
import time
import unicodedata

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
UPSTREAM_REPO = "bodhan-ai/indic-transcribe-flex"
TARGET_REPO = "simple-voice/indic-transcribe-flex-coreml"
ATTRIBUTION = "Built with Indic-Transcribe-Flex from Bodhan AI / AI4Bharat."

# FluidAudio 0.17.4 Canary contract (CanaryConfig.swift, CanaryManager.swift, ModelNames.Canary).
SR = 16000
MAX_SAMPLES = 240_000
N_MELS = 128
MEL_FRAMES = 1501
ENC_FRAMES = 188
D = 1024
DEC_STEPS = 256
EOS, PAD, BOS = 3, 2, 4
VOCAB_SIZE = 7152
MODELS = ("Preprocessor", "EncoderInt4", "DecoderInt4", "Projection")
CONTRACT = {
    "Preprocessor": ({"audio_signal": ("float32", [1, MAX_SAMPLES]), "audio_length": ("int32", [1])},
                     {"processed": [1, N_MELS, MEL_FRAMES], "processed_length": [1]}),
    "EncoderInt4": ({"features": ("float32", [1, N_MELS, MEL_FRAMES]), "features_length": ("int32", [1])},
                    {"encoder": [1, D, ENC_FRAMES], "encoder_length": [1]}),
    "DecoderInt4": ({"input_ids": ("int32", [1, DEC_STEPS]), "decoder_mask": ("float32", [1, DEC_STEPS]),
                     "encoder_embeddings": ("float32", [1, ENC_FRAMES, D]),
                     "encoder_mask": ("float32", [1, ENC_FRAMES])},
                    {"decoder": [1, DEC_STEPS, D]}),
    "Projection": ({"hidden": ("float32", [1, D])}, {"logits": [1, VOCAB_SIZE]}),
}

# Validation thresholds (written into the model card).
CER_MAX_PER_CLIP = 0.10   # character error rate of the int4 Core ML text against the NeMo text, per clip
CER_MAX_MEAN = 0.05       # mean of the above over all clips
LATIN_MIN_ROMANIZED = 0.95  # share of Latin letters among all letters in prompt-(a) Core ML output


def die(msg: str):
    print(f"indic_flex: {msg}", file=sys.stderr, flush=True)
    sys.exit(1)


def sha256(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def jload(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def jdump(obj, path):
    with open(path, "w", encoding="utf-8") as f:
        json.dump(obj, f, indent=2, ensure_ascii=False)
        f.write("\n")


# ---------------------------------------------------------------- memory guard
def _swap_mb():
    out = subprocess.run(["sysctl", "-n", "vm.swapusage"], capture_output=True, text=True, check=True).stdout
    parts = out.replace("M", "").split()
    return float(parts[parts.index("used") + 2]), float(parts[parts.index("total") + 2])


class MemGuard(threading.Thread):
    """Aborts the process (exit 3) when swap grows past the limit, RSS passes the cap, or free disk on the work
    volume falls below the floor."""

    def __init__(self, work, label, growth_mb, max_pct, rss_gb, disk_floor_gb):
        super().__init__(daemon=True)
        import psutil

        self.proc = psutil.Process()
        self.work, self.label = work, label
        self.growth_mb, self.max_pct, self.rss_gb, self.disk_floor_gb = growth_mb, max_pct, rss_gb, disk_floor_gb
        self.peak_rss = self.peak_swap = self.peak_pct = self.peak_sys = 0.0
        self.min_disk_free = float("inf")
        self.tripped = None
        self.stopped = False
        bpath = os.path.join(work, "swap_baseline.json")
        if not os.path.exists(bpath):
            used, total = _swap_mb()
            jdump({"swap_used_mb": used, "swap_total_mb": total, "at": time.strftime("%Y-%m-%dT%H:%M:%S%z")}, bpath)
        self.base = jload(bpath)

    def run(self):
        import shutil

        import psutil

        while not self.stopped:
            rss = self.proc.memory_info().rss
            used, total = _swap_mb()
            pct = 100.0 * used / total if total else 0.0
            vm = psutil.virtual_memory()
            free_disk = shutil.disk_usage(self.work).free
            self.peak_rss = max(self.peak_rss, rss)
            self.peak_swap = max(self.peak_swap, used)
            self.peak_pct = max(self.peak_pct, pct)
            self.peak_sys = max(self.peak_sys, vm.total - vm.available)
            self.min_disk_free = min(self.min_disk_free, free_disk)
            grow = used - self.base["swap_used_mb"]
            capped = self.max_pct > 0 and pct > self.max_pct
            if (grow > self.growth_mb or capped or rss > self.rss_gb * 2**30
                    or free_disk < self.disk_floor_gb * 2**30):
                self.tripped = (f"swap {used:.0f}/{total:.0f} MB ({pct:.1f}%, +{grow:.0f} MB over baseline), "
                                f"rss {rss / 2**30:.2f} GB, disk free {free_disk / 2**30:.1f} GB")
                self.dump()
                print(f"MEMGUARD ABORT: {self.tripped}", file=sys.stderr, flush=True)
                os._exit(3)
            time.sleep(1.0)

    def dump(self):
        path = os.path.join(self.work, "mem_peak.json")
        hist = jload(path) if os.path.exists(path) else {}
        hist[self.label] = {
            "baseline": self.base,
            "limits": {"swap_growth_mb": self.growth_mb, "swap_max_pct": self.max_pct, "rss_max_gb": self.rss_gb,
                       "disk_floor_gb": self.disk_floor_gb},
            "peak_process_rss_gb": round(self.peak_rss / 2**30, 2),
            "peak_system_used_gb": round(self.peak_sys / 2**30, 2),
            "peak_swap_used_mb": round(self.peak_swap),
            "min_disk_free_gb": round(self.min_disk_free / 2**30, 1),
            "peak_swap_pct": round(self.peak_pct, 1),
            "tripped": self.tripped,
        }
        jdump(hist, path)


# ---------------------------------------------------------------- upstream (no NeMo)
def flex_pieces(flex):
    import sentencepiece as spm

    spl = spm.SentencePieceProcessor(model_file=os.path.join(flex, "tokenizer_spl_tokens.model"))
    multi = spm.SentencePieceProcessor(model_file=os.path.join(flex, "tokenizer_multilingual.model"))
    pieces = [spl.id_to_piece(i) for i in range(spl.get_piece_size())]
    pieces += [multi.id_to_piece(i) for i in range(multi.get_piece_size())]
    return pieces, spl.get_piece_size()


def derive_prompts(pieces):
    """10-token canary2 prompts of Flex's tokenizer (tokenization_indic_canary.py): no leading '▁',
    slots <|startofcontext|><|startoftranscript|><|emo:undefined|><|L|><|L|><|pnc|>
    <|noitn|/|itn|><|noromanized|/|romanized|><|notimestamp|><|nodiarize|>."""
    idx = {p: i for i, p in enumerate(pieces)}

    def t(p):
        if p not in idx:
            die(f"prompt token {p} missing from Flex's spl tokenizer")
        return idx[p]

    def build(lang, itn, rom):
        return [t("<|startofcontext|>"), t("<|startoftranscript|>"), t("<|emo:undefined|>"),
                t(f"<|{lang}|>"), t(f"<|{lang}|>"), t("<|pnc|>"),
                t("<|itn|>") if itn else t("<|noitn|>"),
                t("<|romanized|>") if rom else t("<|noromanized|>"),
                t("<|notimestamp|>"), t("<|nodiarize|>")]

    return {
        "a_hi_romanized": build("hi", False, True),
        "b_hi_mixed": build("hi", True, False),
        "c_hi_native": build("hi", False, False),
        "d_en": build("en", False, False),
    }


def licence_use_cap(text):
    m = re.search(r"## 14\. Large-Scale Product Use\s*\n\s*\n(.+?)\n", text)
    if not m:
        die("licence: section 14 (Large-Scale Product Use) not found")
    return m.group(1).strip()


def cmd_upstream(a):
    flex = a.flex
    pieces, spl_size = flex_pieces(flex)
    cfg = jload(os.path.join(flex, "tokenizer_config.json"))
    if len(pieces) != VOCAB_SIZE:
        die(f"Flex vocabulary is {len(pieces)} pieces, expected {VOCAB_SIZE}")
    if cfg["layout"] != {"spl_tokens": [0, spl_size], "multilingual": [spl_size, len(pieces)]}:
        die(f"unexpected tokenizer layout {cfg['layout']}")
    specials = {"eos": pieces.index("<|endoftext|>"), "pad": pieces.index("<pad>"),
                "bos": pieces.index("<|startoftranscript|>")}
    if (specials["eos"], specials["pad"], specials["bos"]) != (EOS, PAD, BOS):
        die(f"special ids {specials} differ from FluidAudio's eos=3 pad=2 bos=4")
    if (cfg["eos_id"], cfg["pad_id"], cfg["bos_id"]) != (EOS, PAD, BOS):
        die("tokenizer_config.json special ids differ from FluidAudio's")
    prompts = derive_prompts(pieces)
    if prompts["c_hi_native"] != cfg["prompt_ids_by_lang"]["hi"]:
        die(f"native hi prompt {prompts['c_hi_native']} != tokenizer_config {cfg['prompt_ids_by_lang']['hi']}")

    # vocab.json MUST be a JSON object {"id": "piece"}; FluidAudio's Tokenizer force-casts to [String: String].
    vocab = {str(i): p for i, p in enumerate(pieces)}
    os.makedirs(a.work, exist_ok=True)
    jdump(vocab, os.path.join(a.work, "vocab.json"))
    back = jload(os.path.join(a.work, "vocab.json"))
    if not isinstance(back, dict) or len(back) != VOCAB_SIZE or not all(isinstance(v, str) for v in back.values()):
        die("vocab.json is not a 7152-entry {id: piece} object")

    lic_path = os.path.join(flex, "Bodhan_AI_Open_Model_License.md")
    lic = open(lic_path, encoding="utf-8").read()
    if not lic.startswith("# INDIC OPEN MODEL LICENSE") or "Version: 1.0" not in lic[:200]:
        die("Bodhan_AI_Open_Model_License.md is not the Indic Open Model License v1.0")
    if '"Built with [Model Name] from Bodhan AI / AI4Bharat."' not in lic:
        die("licence attribution notice text changed; re-read the licence before publishing")
    if "must be licensed under this exact License" not in lic:
        die("licence share-alike clause (section 5.1) changed; re-read the licence before publishing")

    card = open(os.path.join(flex, "README.md"), encoding="utf-8").read()
    fm = re.match(r"---\n(.*?)\n---\n", card, re.S)
    langs = re.findall(r"^\s*-\s*([a-z]{2,3})\s*$", fm.group(1).split("language:")[1].split(":")[0], re.M) if fm else []
    if "hi" not in langs or "en" not in langs:
        die(f"could not read languages from the upstream card front matter: {langs}")

    up = {"revision": a.revision, "vocab_size": len(pieces), "specials": specials, "prompts": prompts,
          "languages": langs, "licence_file_sha256": sha256(lic_path), "licence_use_cap": licence_use_cap(lic),
          "vocab_piece0": pieces[0]}
    jdump(up, os.path.join(a.work, "upstream.json"))
    print(json.dumps({"vocab_size": len(pieces), "specials": specials, "prompts": prompts}, ensure_ascii=False))


# ---------------------------------------------------------------- NeMo (reference, trace)
def load_nemo(work):
    """Restore the checkpoint with the lowest RAM peak on CPU:
      - publish.sh pre-extracts the .nemo into WORK/nemo (no temp-dir unpack);
      - the model's parameters are built on the meta device (no 4.9 GB fp32 random initialisation);
      - the weights are torch.load(mmap=True) (file-backed pages, not swap) and attached with
        load_state_dict(assign=True), so nothing is copied;
      - buffers stay on CPU; any meta buffer NeMo can rebuild (the relative positional table) is recomputed,
        and any other tensor left on meta fails the load.
    Weights stay fp32: the encoder's depthwise convolutions and layer norms are traced on CPU, where fp16
    kernels are not exact, and coremltools casts to fp16 itself."""
    import torch
    from nemo.core.classes.common import typecheck
    from nemo.utils.app_state import AppState
    from omegaconf import OmegaConf

    flex = os.path.join(work, "flex")
    sys.path.insert(0, os.path.join(flex, "nemo"))
    from load_nemo import register_tokenizer

    register_tokenizer()
    from nemo.collections.asr.models import EncDecMultiTaskModel

    extracted = os.path.join(work, "nemo")
    ckpt = os.path.join(extracted, "model_weights.ckpt")
    if not os.path.isfile(ckpt):
        die(f"{extracted} has no model_weights.ckpt; publish.sh extracts the .nemo there first")
    typecheck.set_typecheck_enabled(False)
    torch.set_grad_enabled(False)

    conf = OmegaConf.load(os.path.join(extracted, "model_config.yaml"))
    conf = conf.model if "model" in conf else conf
    OmegaConf.set_struct(conf, True)
    AppState().model_restore_path = extracted  # resolves the config's "nemo:" tokenizer paths
    EncDecMultiTaskModel._set_model_restore_state(is_being_restored=True, folder=extracted)
    cwd = os.getcwd()
    os.chdir(extracted)
    # Parameters go to the meta device as each module registers them, before its random initialisation runs;
    # buffers and scratch tensors stay on CPU (NeMo's constructors compute shapes with .item() on CPU tensors,
    # which a blanket `with torch.device("meta")` breaks).
    register = torch.nn.Module.register_parameter

    def register_on_meta(module, name, param):
        register(module, name, param)
        if param is not None and not param.is_meta:
            module._parameters[name] = torch.nn.Parameter(param.to("meta"), requires_grad=param.requires_grad)

    torch.nn.Module.register_parameter = register_on_meta
    try:
        m = EncDecMultiTaskModel.from_config_dict(config=conf, trainer=None)
    finally:
        torch.nn.Module.register_parameter = register
        os.chdir(cwd)
    if any(not p.is_meta for p in m.parameters()):
        die("model build left parameters off the meta device")
    state = torch.load(ckpt, map_location="cpu", mmap=True, weights_only=True)
    m.load_state_dict(state, strict=True, assign=True)
    m._set_model_restore_state(is_being_restored=False)
    del state

    # Non-persistent buffers are not in the checkpoint: recompute the ones NeMo can rebuild, refuse the rest.
    for mod in m.modules():
        stale = [n for n, b in mod.named_buffers(recurse=False) if b is not None and b.is_meta]
        if not stale:
            continue
        if hasattr(mod, "extend_pe") and stale == ["pe"]:
            del mod._buffers["pe"]
            mod.extend_pe(mod.max_len, torch.device("cpu"), torch.float32)
    left = [n for n, t in list(m.named_parameters()) + list(m.named_buffers()) if t.is_meta]
    if left:
        die(f"tensors still on the meta device after loading: {left[:20]}")
    m.eval()
    m.preprocessor.featurizer.dither = 0.0
    m.preprocessor.featurizer.pad_to = 0
    if m.use_transf_encoder:
        die("Flex has a transformer encoder; FluidAudio's Canary layout has no slot for it")
    if m.log_softmax.mlp.layer0.out_features != VOCAB_SIZE:
        die(f"head has {m.log_softmax.mlp.layer0.out_features} classes, expected {VOCAB_SIZE}")
    return m


def read_clip(path):
    import soundfile as sf

    audio, sr = sf.read(path, dtype="float32")
    if sr != SR or audio.ndim != 1:
        die(f"{path}: need 16 kHz mono, got {sr} Hz shape {audio.shape}")
    return audio[:MAX_SAMPLES]


def clip_list():
    rows = []
    with open(os.path.join(HERE, "clips", "clips.tsv"), encoding="utf-8") as f:
        for line in f.read().splitlines():
            name, voice, text = line.split("|", 2)
            rows.append((name, voice, text))
    return rows


def clip_prompt(prompts, name):
    return prompts["d_en"] if name.startswith("english") else prompts["a_hi_romanized"]


def detok(pieces, ids):
    text = "".join(pieces[i] for i in ids).replace("\u2581", " ")
    return re.sub(r"<\|[^|]*\|>", "", text).strip()  # same as CanaryManager.detokenize


def torch_encode(m, audio):
    import torch

    sig = torch.zeros(1, MAX_SAMPLES)
    sig[0, : len(audio)] = torch.from_numpy(audio)
    mel, mel_len = m.preprocessor(input_signal=sig, length=torch.tensor([len(audio)]))
    enc, enc_len = m.encoder(audio_signal=mel, length=mel_len)
    return mel, mel_len, m.encoder_decoder_proj(enc.transpose(1, 2)), enc_len


def torch_greedy(m, emb, enc_len, prompt):
    """No-KV-cache, full-sequence greedy loop over NeMo's own decoder and head (argmax of the linear)."""
    import torch

    proj = m.log_softmax.mlp.layer0
    enc_mask = (torch.arange(emb.shape[1])[None, :] < enc_len[:, None]).float()
    ids = torch.zeros(1, DEC_STEPS, dtype=torch.long)
    msk = torch.zeros(1, DEC_STEPS)
    ids[0, : len(prompt)] = torch.tensor(prompt)
    msk[0, : len(prompt)] = 1
    pos, out = len(prompt), []
    while pos < DEC_STEPS:
        h = m.transf_decoder(input_ids=ids, decoder_mask=msk, encoder_embeddings=emb, encoder_mask=enc_mask)
        nxt = int(torch.argmax(proj(h[0, pos - 1])).item())
        if nxt == EOS:
            break
        out.append(nxt)
        ids[0, pos] = nxt
        msk[0, pos] = 1
        pos += 1
    return out


def reference(m, work, pieces, prompts):
    res = {}
    for name, voice, spoken in clip_list():
        audio = read_clip(os.path.join(HERE, "clips", f"{name}.wav"))
        prompt = clip_prompt(prompts, name)
        t0 = time.perf_counter()
        _, _, emb, enc_len = torch_encode(m, audio)
        ids = torch_greedy(m, emb, enc_len, prompt)
        res[name] = {"voice": voice, "spoken": spoken, "seconds": round(len(audio) / SR, 2), "prompt": prompt,
                     "ids": ids, "text": detok(pieces, ids), "latency_s": round(time.perf_counter() - t0, 2)}
        print(f"nemo {name}: {res[name]['text']}", flush=True)
    jdump(res, os.path.join(work, "reference.json"))


# Core ML parts: traced input example kind, inputs, outputs, precision, target, compute units.
PARTS = ("Preprocessor", "Encoder", "Decoder", "Projection")


def part_spec(name):
    import coremltools as ct

    tt = lambda n, shape, dt: ct.TensorType(name=n, shape=shape, dtype=dt)
    return {
        "Preprocessor": ([tt("audio_signal", (1, MAX_SAMPLES), np.float32), tt("audio_length", (1,), np.int32)],
                         ["processed", "processed_length"], ct.precision.FLOAT32, ct.target.iOS17,
                         ct.ComputeUnit.CPU_ONLY),
        "Encoder": ([tt("features", (1, N_MELS, MEL_FRAMES), np.float32), tt("features_length", (1,), np.int32)],
                    ["encoder", "encoder_length"], ct.precision.FLOAT16, ct.target.iOS18, ct.ComputeUnit.CPU_AND_NE),
        "Decoder": ([tt("input_ids", (1, DEC_STEPS), np.int32), tt("decoder_mask", (1, DEC_STEPS), np.float32),
                     tt("encoder_embeddings", (1, ENC_FRAMES, D), np.float32),
                     tt("encoder_mask", (1, ENC_FRAMES), np.float32)],
                    ["decoder"], ct.precision.FLOAT16, ct.target.iOS18, ct.ComputeUnit.CPU_AND_NE),
        "Projection": ([tt("hidden", (1, D), np.float32)], ["logits"], ct.precision.FLOAT16, ct.target.iOS17,
                       ct.ComputeUnit.CPU_AND_NE),
    }[name]


class Guarded:
    def __init__(self, a, label):
        self.g = MemGuard(a.work, label, a.swap_growth_mb, a.swap_max_pct, a.rss_max_gb, a.disk_floor_gb)

    def __enter__(self):
        self.g.start()

    def __exit__(self, *exc):
        self.g.stopped = True
        self.g.dump()


def cmd_reference(a):
    """NeMo PyTorch greedy decode of every clip (fp32, no KV cache) -> reference.json."""
    with Guarded(a, "reference"):
        up = jload(os.path.join(a.work, "upstream.json"))
        pieces = [jload(os.path.join(a.work, "vocab.json"))[str(i)] for i in range(VOCAB_SIZE)]
        m = load_nemo(a.work)
        res = {}
        for name, voice, spoken in clip_list():
            audio = read_clip(os.path.join(HERE, "clips", f"{name}.wav"))
            prompt = clip_prompt(up["prompts"], name)
            t0 = time.perf_counter()
            _, _, emb, enc_len = torch_encode(m, audio)
            ids = torch_greedy(m, emb, enc_len, prompt)
            res[name] = {"voice": voice, "spoken": spoken, "seconds": round(len(audio) / SR, 2), "prompt": prompt,
                         "ids": ids, "text": detok(pieces, ids), "latency_s": round(time.perf_counter() - t0, 2)}
            print(f"nemo {name}: {res[name]['text']}", flush=True)
        jdump(res, os.path.join(a.work, "reference.json"))


def cmd_trace(a):
    """One NeMo load per part: keep only that part's submodules, drop the rest of the model, trace, save .pt."""
    import torch

    name = a.part
    dst = os.path.join(a.work, f"{name}.pt")
    with Guarded(a, f"trace-{name}"):
        m = load_nemo(a.work)
        audio = read_clip(os.path.join(HERE, "clips", f"{clip_list()[0][0]}.wav"))
        if name == "Preprocessor":
            f = m.preprocessor.featurizer

            class Part(torch.nn.Module):
                def __init__(self):
                    super().__init__()
                    self.f = f

                def forward(self, audio_signal, audio_length):
                    mel, ln = self.f(audio_signal, audio_length.long())
                    return mel, ln.int()

            sig = torch.zeros(1, MAX_SAMPLES)
            sig[0, : len(audio)] = torch.from_numpy(audio)
            example = (sig, torch.tensor([len(audio)], dtype=torch.int32))
        elif name == "Encoder":
            mel, mel_len, _, _ = torch_encode(m, audio)
            if mel.shape[-1] != MEL_FRAMES:
                die(f"preprocessor gives {mel.shape[-1]} frames for a 15 s window, expected {MEL_FRAMES}")
            e, p = m.encoder, m.encoder_decoder_proj

            class Part(torch.nn.Module):
                def __init__(self):
                    super().__init__()
                    self.e, self.p = e, p

                def forward(self, features, features_length):
                    enc, ln = self.e(audio_signal=features, length=features_length.long())
                    return self.p(enc.transpose(1, 2)).transpose(1, 2), ln.int()

            example = (mel, mel_len.int())
        elif name == "Decoder":
            d = m.transf_decoder

            class Part(torch.nn.Module):
                def __init__(self):
                    super().__init__()
                    self.d = d

                def forward(self, input_ids, decoder_mask, encoder_embeddings, encoder_mask):
                    return self.d(input_ids=input_ids.long(), decoder_mask=decoder_mask,
                                  encoder_embeddings=encoder_embeddings, encoder_mask=encoder_mask)

            ids = torch.zeros(1, DEC_STEPS, dtype=torch.int32)
            ids[0, :10] = torch.tensor(jload(os.path.join(a.work, "upstream.json"))["prompts"]["a_hi_romanized"])
            msk = torch.zeros(1, DEC_STEPS)
            msk[0, :10] = 1
            example = (ids, msk, torch.randn(1, ENC_FRAMES, D), torch.ones(1, ENC_FRAMES))
        else:
            lin = m.log_softmax.mlp.layer0

            class Part(torch.nn.Module):
                def __init__(self):
                    super().__init__()
                    self.l = lin

                def forward(self, hidden):
                    return self.l(hidden)

            example = (torch.zeros(1, D),)
        part = Part().eval()
        del m  # only the part's submodules stay referenced
        gc.collect()
        traced = torch.jit.trace(part, example, check_trace=False, strict=False)
        torch.jit.save(traced, dst)
        print(f"trace: {name} -> {dst}", flush=True)


def cmd_coreml(a):
    """Convert one traced part to an .mlpackage without NeMo or the eager model in memory."""
    import coremltools as ct
    import torch

    name = a.part
    src, dst = os.path.join(a.work, f"{name}.pt"), os.path.join(a.work, f"{name}.mlpackage")
    with Guarded(a, f"coreml-{name}"):
        ins, outs, prec, tgt, cu = part_spec(name)
        traced = torch.jit.load(src, map_location="cpu").eval()
        t0 = time.time()
        mlm = ct.convert(traced, inputs=ins, outputs=[ct.TensorType(name=o) for o in outs],
                         compute_precision=prec, minimum_deployment_target=tgt, compute_units=cu,
                         convert_to="mlprogram", skip_model_load=True)
        del traced
        gc.collect()
        mlm.short_description = f"Indic-Transcribe-Flex {name} (FluidAudio Canary layout)"
        mlm.save(dst)
        print(f"coreml: {name} in {time.time() - t0:.0f}s -> {dst}", flush=True)


def cmd_quantize(a):
    """int4 linear_symmetric per_block 32 on one of Encoder / Decoder -> <Part>Int4.mlpackage."""
    import coremltools as ct
    import coremltools.optimize as cto

    name = a.part
    if name not in ("Encoder", "Decoder"):
        die("quantize: --part must be Encoder or Decoder")
    with Guarded(a, f"quantize-{name}"):
        cfg = cto.coreml.OptimizationConfig(global_config=cto.coreml.OpLinearQuantizerConfig(
            mode="linear_symmetric", dtype="int4", granularity="per_block", block_size=32))
        t0 = time.time()
        mlm = ct.models.MLModel(os.path.join(a.work, f"{name}.mlpackage"), skip_model_load=True)
        q = cto.coreml.linear_quantize_weights(mlm, config=cfg)
        del mlm
        gc.collect()
        q.save(os.path.join(a.work, f"{name}Int4.mlpackage"))
        print(f"quantize: {name}Int4 in {time.time() - t0:.0f}s", flush=True)


def cmd_contract(a):
    import coremltools as ct

    dt = {ct.proto.FeatureTypes_pb2.ArrayFeatureType.FLOAT32: "float32",
          ct.proto.FeatureTypes_pb2.ArrayFeatureType.INT32: "int32",
          ct.proto.FeatureTypes_pb2.ArrayFeatureType.FLOAT16: "float16"}
    bad = []
    for name in MODELS:
        pkg = os.path.join(a.work, f"{name}.mlpackage")
        if not os.path.isdir(pkg) and os.path.isdir(os.path.join(a.work, "compiled", f"{name}.mlmodelc")):
            continue  # checked before an earlier compile consumed (and deleted) the package
        spec = ct.models.MLModel(pkg, skip_model_load=True).get_spec()
        ins = {i.name: (dt.get(i.type.multiArrayType.dataType), list(i.type.multiArrayType.shape))
               for i in spec.description.input}
        outs = {o.name for o in spec.description.output}
        want_in, want_out = CONTRACT[name]
        if ins != {k: (v[0], v[1]) for k, v in want_in.items()}:
            bad.append(f"{name} inputs {ins} != {want_in}")
        if outs != set(want_out):
            bad.append(f"{name} outputs {sorted(outs)} != {sorted(want_out)}")
    if bad:
        die("contract mismatch:\n  " + "\n  ".join(bad))
    print("contract: names, dtypes and input shapes match FluidAudio 0.17.4", flush=True)


# ---------------------------------------------------------------- validate (Core ML)
def cer(hyp, ref):
    h, r = list(hyp), list(ref)
    if not r:
        return 0.0 if not h else 1.0
    prev = list(range(len(h) + 1))
    for i, rc in enumerate(r, 1):
        cur = [i] + [0] * len(h)
        for j, hc in enumerate(h, 1):
            cur[j] = min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (rc != hc))
        prev = cur
    return prev[-1] / len(r)


def latin_share(text):
    letters = [c for c in text if c.isalpha()]
    if not letters:
        return 0.0
    return sum(1 for c in letters if "LATIN" in unicodedata.name(c, "")) / len(letters)


class CoreMLCanary:
    """Python port of FluidAudio 0.17.4 CanaryManager.transcribeWindow (fixed 15 s window)."""

    def __init__(self, models_dir):
        import coremltools as ct

        load = lambda n, cu: ct.models.CompiledMLModel(os.path.join(models_dir, f"{n}.mlmodelc"), cu)
        self.pre = load("Preprocessor", ct.ComputeUnit.CPU_ONLY)
        self.enc = load("EncoderInt4", ct.ComputeUnit.CPU_AND_NE)
        self.dec = load("DecoderInt4", ct.ComputeUnit.CPU_AND_NE)
        self.proj = load("Projection", ct.ComputeUnit.CPU_AND_NE)

    def transcribe_ids(self, audio, prompt):
        n = min(len(audio), MAX_SAMPLES)
        sig = np.zeros((1, MAX_SAMPLES), np.float32)
        sig[0, :n] = audio[:n]
        p = self.pre.predict({"audio_signal": sig, "audio_length": np.array([n], np.int32)})
        e = self.enc.predict({"features": p["processed"].astype(np.float32),
                              "features_length": p["processed_length"].astype(np.int32).reshape(1)})
        enc_len = int(np.asarray(e["encoder_length"]).reshape(-1)[0])
        emb = np.ascontiguousarray(np.transpose(e["encoder"], (0, 2, 1))).astype(np.float32)
        emask = (np.arange(ENC_FRAMES) < min(max(enc_len, 1), ENC_FRAMES)).astype(np.float32)[None, :]
        ids = np.zeros((1, DEC_STEPS), np.int32)
        msk = np.zeros((1, DEC_STEPS), np.float32)
        ids[0, : len(prompt)] = prompt
        msk[0, : len(prompt)] = 1
        pos, out = len(prompt), []
        while pos < DEC_STEPS:
            h = self.dec.predict({"input_ids": ids, "decoder_mask": msk, "encoder_embeddings": emb,
                                  "encoder_mask": emask})["decoder"]
            logits = self.proj.predict({"hidden": h[0, pos - 1][None, :].astype(np.float32)})["logits"]
            if logits.shape[-1] != VOCAB_SIZE:
                die(f"projection gives {logits.shape[-1]} logits, expected {VOCAB_SIZE}")
            nxt = int(np.argmax(logits.reshape(-1)))
            if nxt == EOS:
                break
            out.append(nxt)
            ids[0, pos] = nxt
            msk[0, pos] = 1
            pos += 1
        return out


def cmd_compile(a):
    """.mlpackage -> .mlmodelc with the Core ML framework's own compiler (MLModel.compileModel; needs no Xcode).
    Each compiled model is verified (loads; int4 blockwise weights in the encoder and decoder) and its
    .mlpackage is then deleted, unless --keep."""
    import shutil

    import coremltools as ct

    out = os.path.join(a.work, "compiled")
    os.makedirs(out, exist_ok=True)
    with Guarded(a, "compile"):
        for name in MODELS:
            src, dst = os.path.join(a.work, f"{name}.mlpackage"), os.path.join(out, f"{name}.mlmodelc")
            if not os.path.isdir(dst):
                ct.utils.compile_model(src, dst)
                print(f"compile: {name}.mlmodelc", flush=True)
            if name in ("EncoderInt4", "DecoderInt4"):
                with open(os.path.join(dst, "model.mil"), encoding="utf-8") as f:
                    if "constexpr_blockwise_shift_scale" not in f.read():
                        die(f"{name} has no int4 blockwise weights")
            ct.models.CompiledMLModel(dst, ct.ComputeUnit.CPU_ONLY)  # loads, or raises
            if not a.keep and os.path.isdir(src):
                shutil.rmtree(src)


def cmd_validate(a):
    with Guarded(a, "validate"):
        _validate(a)


def _validate(a):
    up = jload(os.path.join(a.work, "upstream.json"))
    ref = jload(os.path.join(a.work, "reference.json"))
    pieces = [jload(os.path.join(a.work, "vocab.json"))[str(i)] for i in range(VOCAB_SIZE)]
    cm = CoreMLCanary(os.path.join(a.work, "compiled"))
    rows, fails = [], []
    for name, voice, spoken in clip_list():
        audio = read_clip(os.path.join(HERE, "clips", f"{name}.wav"))
        prompt = clip_prompt(up["prompts"], name)
        cm.transcribe_ids(audio, prompt)  # warm-up (first ANE run compiles)
        t0 = time.perf_counter()
        ids = cm.transcribe_ids(audio, prompt)
        dt = time.perf_counter() - t0
        text = detok(pieces, ids)
        nemo = ref[name]["text"]
        row = {"clip": name, "voice": voice, "spoken": spoken, "seconds": ref[name]["seconds"],
               "prompt": "d en" if name.startswith("english") else "a hi romanized",
               "nemo_text": nemo, "coreml_int4_text": text, "cer_vs_nemo": round(cer(text, nemo), 4),
               "nemo_latency_s": ref[name]["latency_s"], "coreml_latency_s": round(dt, 2)}
        if not text:
            fails.append(f"{name}: empty Core ML output")
        if row["cer_vs_nemo"] > CER_MAX_PER_CLIP:
            fails.append(f"{name}: CER {row['cer_vs_nemo']} > {CER_MAX_PER_CLIP}")
        if not name.startswith("english"):
            row["latin_share"] = round(latin_share(text), 3)
            if row["latin_share"] < LATIN_MIN_ROMANIZED:
                fails.append(f"{name}: Latin share {row['latin_share']} < {LATIN_MIN_ROMANIZED} (not romanized)")
        rows.append(row)
        print(f"coreml {name}: {text}  (CER {row['cer_vs_nemo']})", flush=True)
    mean = sum(r["cer_vs_nemo"] for r in rows) / len(rows)
    if mean > CER_MAX_MEAN:
        fails.append(f"mean CER {mean:.4f} > {CER_MAX_MEAN}")
    result = {"rows": rows, "mean_cer": round(mean, 4), "passed": not fails, "failures": fails,
              "thresholds": {"cer_max_per_clip": CER_MAX_PER_CLIP, "cer_max_mean": CER_MAX_MEAN,
                             "latin_min_romanized": LATIN_MIN_ROMANIZED}}
    jdump(result, os.path.join(a.work, "validation.json"))
    if fails:
        die("validation FAILED:\n  " + "\n  ".join(fails))
    print(f"validation passed: mean CER {mean:.4f}", flush=True)


# ---------------------------------------------------------------- stage
def tree_size(path):
    if os.path.isfile(path):
        return os.path.getsize(path)
    return sum(os.path.getsize(os.path.join(r, f)) for r, _, fs in os.walk(path) for f in fs)


def card_text(up, val, sizes, prov):
    rev = up["revision"]
    langs = "\n".join(f"- {l}" for l in up["languages"])
    p = up["prompts"]
    mb = lambda n: f"{n / 1e6:.1f} MB"
    shapes = {
        "Preprocessor.mlmodelc": ("waveform `[1,240000]` fp32 → mel `processed` `[1,128,1501]`", "fp32, CPU"),
        "EncoderInt4.mlmodelc": ("mel → `encoder` `[1,1024,188]`", "int4 per-block-32 symmetric, iOS 18 / macOS 15"),
        "DecoderInt4.mlmodelc": ("`input_ids` `[1,256]` + encoder context → hidden `decoder` `[1,256,1024]` (no KV cache)",
                                 "int4 per-block-32 symmetric, iOS 18 / macOS 15"),
        "Projection.mlmodelc": ("hidden `[1,1024]` → `logits` `[1,7152]`", "fp16"),
        "vocab.json": ("7152 SentencePiece pieces, JSON object `{\"id\": \"piece\"}`", "—"),
        "metadata.json": ("shapes, special token ids, prompts, upstream revision", "—"),
    }
    files = "\n".join(f"| `{f}` | {d} | {q} | {mb(sizes[f])} |" for f, (d, q) in shapes.items())
    total = sum(sizes[f] for f in shapes)
    vrows = "\n".join(
        f"| {r['clip']} ({r['voice']}, {r['seconds']} s) | {r['prompt']} | {r['nemo_text']} | {r['coreml_int4_text']} | "
        f"{r['cer_vs_nemo']:.3f} | {r['nemo_latency_s']} s | {r['coreml_latency_s']} s |" for r in val["rows"])
    th = val["thresholds"]
    return f"""---
license: other
license_name: indic-open-model-license-v1
license_link: https://huggingface.co/{TARGET_REPO}/blob/main/Indic_Open_Model_License.md
base_model: {UPSTREAM_REPO}
base_model_relation: quantized
pipeline_tag: automatic-speech-recognition
library_name: coreml
language:
{langs}
tags:
- coreml
- apple-silicon
- ane
- fluidaudio
- canary
- indic
- hinglish
- romanization
---

# Indic-Transcribe-Flex — Core ML int4 (FluidAudio Canary layout)

**{ATTRIBUTION}**

Core ML conversion of [`{UPSTREAM_REPO}`](https://huggingface.co/{UPSTREAM_REPO}) at revision
`{rev}`, packaged for FluidAudio's `CanaryManager`; used by Simple Voice, where it is the
default Hindi / Hinglish model ("Indic (Hinglish)").

Indic-Transcribe-Flex is Bodhan AI / AI4Bharat's fine-tune of
[NVIDIA Canary-1B-v2](https://huggingface.co/nvidia/canary-1b-v2) (CC BY 4.0): 27 languages, with native-script,
mixed-script or romanized output chosen by a prompt token.

## Licence

This is a Derivative of Indic-Transcribe-Flex and is licensed under the **Indic Open Model License v1.0**
(full text: [`Indic_Open_Model_License.md`](Indic_Open_Model_License.md); attribution: [`NOTICE.md`](NOTICE.md)).

- **Share-alike:** any further derivative you make available must stay under the Indic Open Model License v1.0
  (section 5.1), with the attribution notice above (section 2).
- **Use cap (section 14, verbatim):** "{up['licence_use_cap']}"
- Canary-1B-v2, the upstream architecture and base weights, is by NVIDIA under CC BY 4.0.

## Files

| File | Role | Precision | Size |
|------|------|-----------|------|
{files}

Total: {mb(total)} ({total} bytes). Contract: 15 s window (240000 samples at 16 kHz), 256 decoder steps,
`eos=3`, `pad=2`, `bos=4`, vocabulary 7152. Same feature names and shapes as FluidAudio 0.17.4's Canary loader.

## Prompts (10 tokens, pass as `[Int32]` to `CanaryManager(models:prompt:)`)

Layout `<|startofcontext|> <|startoftranscript|> <|emo:undefined|> <|L|> <|L|> <|pnc|> <|noitn|/|itn|>
<|noromanized|/|romanized|> <|notimestamp|> <|nodiarize|>`, ids from Flex's `tokenizer_spl_tokens.model`
(no leading `▁` piece; (c) equals `prompt_ids_by_lang.hi` in Flex's `tokenizer_config.json`).

| Prompt | Token ids |
|--------|-----------|
| (a) Hindi, romanized (Roman Hinglish), punctuation | `{p['a_hi_romanized']}` |
| (b) Hindi, mixed script (ITN) | `{p['b_hi_mixed']}` |
| (c) Hindi, native script | `{p['c_hi_native']}` |
| (d) English | `{p['d_en']}` |

```swift
let models = try CanaryModels.load(from: directory, precision: .int4)
let manager = CanaryManager(models: models, prompt: {p['a_hi_romanized']})
let text = try await manager.transcribe(audioURL: url)
```

## Validation

Greedy decode with FluidAudio's algorithm (int4 Core ML, CPU+ANE) against NeMo's PyTorch greedy decode
(same prompt, no KV cache), on the fixed clips in `scripts/models/indic-flex/clips/` (macOS `say`, 16 kHz).
Pass thresholds: CER against NeMo ≤ {th['cer_max_per_clip']} per clip and ≤ {th['cer_max_mean']} on average;
romanized-prompt output ≥ {th['latin_min_romanized']:.0%} Latin letters. Mean CER: {val['mean_cer']}.

| Clip | Prompt | NeMo (PyTorch) | Core ML int4 | CER | NeMo latency | Core ML latency |
|------|--------|----------------|--------------|-----|--------------|-----------------|
{vrows}

## Conversion

- Script: `scripts/models/indic-flex/publish.sh {rev}` in yuyudhan/simple-voice ({prov['simple_voice_ref']}),
  uv-locked environment (Python 3.11, torch 2.7.0, coremltools 9.0, nemo_toolkit 2.7.3).
- Decode algorithm reference: FluidInference/mobius `{prov['mobius_commit']}`
  (`models/stt/canary-1b-v2/coreml/torch_reference.py`; that folder ships no conversion scripts).
- Steps: NeMo restore → `torch.jit.trace` of the featurizer, FastConformer encoder, transformer decoder
  (full sequence, `[1,256]`) and the 1024→7152 head → `coremltools.convert` (mlprogram; preprocessor fp32 iOS 17,
  encoder/decoder fp16 iOS 18, projection fp16 iOS 17) → `linear_quantize_weights` int4 `linear_symmetric`
  `per_block` 32 on encoder and decoder → compiled to `.mlmodelc` with Core ML's `MLModel.compileModel`.
"""


def cmd_stage(a):
    import shutil

    up = jload(os.path.join(a.work, "upstream.json"))
    val = jload(os.path.join(a.work, "validation.json"))
    if not val["passed"]:
        die("validation did not pass; refusing to stage")
    out = os.path.join(a.work, "repo")
    if os.path.exists(out):
        shutil.rmtree(out)
    os.makedirs(out)
    for n in MODELS:
        shutil.copytree(os.path.join(a.work, "compiled", f"{n}.mlmodelc"), os.path.join(out, f"{n}.mlmodelc"))
    shutil.copy2(os.path.join(a.work, "vocab.json"), os.path.join(out, "vocab.json"))
    jdump({"model_id": UPSTREAM_REPO, "upstream_revision": up["revision"], "sample_rate": SR,
           "max_audio_seconds": 15.0, "max_audio_samples": MAX_SAMPLES, "max_decoder_seq_length": DEC_STEPS,
           "vocab_size": VOCAB_SIZE, "encoder_hidden": D, "decoder_hidden": D, "encoder_frames": ENC_FRAMES,
           "mel_shape": [1, N_MELS, MEL_FRAMES], "encoder_shape": [1, D, ENC_FRAMES],
           "eos_id": EOS, "pad_id": PAD, "bos_id": BOS, "prompts": up["prompts"],
           "coreml": {"encoder_decoder": "int4 linear_symmetric per_block 32", "projection": "fp16",
                      "preprocessor": "fp32 CPU", "deployment_target": "iOS18"}},
          os.path.join(out, "metadata.json"))
    # Verbatim licence text, as shipped in the upstream revision.
    shutil.copy2(os.path.join(a.flex, "Bodhan_AI_Open_Model_License.md"), os.path.join(out, "Indic_Open_Model_License.md"))
    with open(os.path.join(out, "NOTICE.md"), "w", encoding="utf-8") as f:
        f.write(f"""# NOTICE

{ATTRIBUTION}

This repository contains a Derivative (Core ML conversion, int4 quantization) of Indic-Transcribe-Flex
(`{UPSTREAM_REPO}`, revision `{up['revision']}`), Copyright © 2026 IITM BODHAN-AI FOUNDATION, licensed under the
Indic Open Model License v1.0 (see `Indic_Open_Model_License.md`). The Derivative is distributed under the same
licence.

Indic-Transcribe-Flex is built on NVIDIA Canary-1B-v2 (https://huggingface.co/nvidia/canary-1b-v2), licensed under
CC BY 4.0.
""")
    sizes = {n: tree_size(os.path.join(out, n)) for n in os.listdir(out)}
    prov = {"mobius_commit": a.mobius_commit, "simple_voice_ref": a.simple_voice_ref}
    with open(os.path.join(out, "README.md"), "w", encoding="utf-8") as f:
        f.write(card_text(up, val, sizes, prov))
    manifest = {}
    for root, _, fs in os.walk(out):
        for fn in sorted(fs):
            p = os.path.join(root, fn)
            manifest[os.path.relpath(p, out)] = {"bytes": os.path.getsize(p), "sha256": sha256(p)}
    jdump(dict(sorted(manifest.items())), os.path.join(a.work, "manifest.json"))
    print(f"stage: {len(manifest)} files, {sum(v['bytes'] for v in manifest.values())} bytes in {out}", flush=True)


# ---------------------------------------------------------------- publish
def cmd_publish(a):
    from huggingface_hub import CommitOperationAdd, CommitOperationDelete, HfApi

    api = HfApi()
    up = jload(os.path.join(a.work, "upstream.json"))
    manifest = jload(os.path.join(a.work, "manifest.json"))
    out = os.path.join(a.work, "repo")
    tag = f"flex-{up['revision'][:12]}"
    who = api.whoami()
    org = a.repo.split("/")[0]
    if org != who["name"] and org not in {o["name"] for o in who.get("orgs", [])}:
        die(f"logged-in account {who['name']} is not a member of {org}; not publishing elsewhere")
    api.create_repo(a.repo, repo_type="model", private=False, exist_ok=True)
    api.update_repo_settings(a.repo, private=False, gated=False)
    remote = set(api.list_repo_files(a.repo))
    ops = [CommitOperationAdd(path_in_repo=k, path_or_fileobj=os.path.join(out, k)) for k in manifest]
    ops += [CommitOperationDelete(path_in_repo=k) for k in sorted(remote - set(manifest) - {".gitattributes"})]
    info = api.create_commit(a.repo, operations=ops,
                             commit_message=f"Indic-Transcribe-Flex {up['revision'][:12]} → Core ML int4")
    commit = info.oid
    refs = {t.name: t.target_commit for t in api.list_repo_refs(a.repo).tags}
    if tag in refs and refs[tag] != commit:
        die(f"tag {tag} already points at {refs[tag]}, not {commit}; remove it deliberately before re-tagging")
    if tag not in refs:
        api.create_tag(a.repo, tag=tag, revision=commit)
    # Remote integrity: LFS files carry their SHA-256; the rest are fetched anonymously and hashed.
    import urllib.request

    remote_info = {p.path: p for p in api.get_paths_info(a.repo, list(manifest), revision=commit)}
    for k, v in manifest.items():
        if k not in remote_info:
            die(f"remote is missing {k}")
        lfs = getattr(remote_info[k], "lfs", None)
        if lfs is not None:
            got = lfs.sha256
        else:
            url = f"https://huggingface.co/{a.repo}/resolve/{commit}/{urllib.request.quote(k)}"
            with urllib.request.urlopen(urllib.request.Request(url)) as r:  # no Authorization header
                got = hashlib.sha256(r.read()).hexdigest()
        if got != v["sha256"]:
            die(f"remote {k} sha256 {got} != local {v['sha256']}")
    with urllib.request.urlopen(f"https://huggingface.co/api/models/{a.repo}") as r:  # anonymous
        meta = json.load(r)
    if meta.get("private") or meta.get("gated"):
        die(f"repo is not public and ungated: private={meta.get('private')} gated={meta.get('gated')}")
    jdump({"repo": a.repo, "commit": commit, "tag": tag, "upstream_revision": up["revision"]},
          os.path.join(a.work, "publish.json"))
    print(json.dumps({"repo": a.repo, "commit": commit, "tag": tag}), flush=True)


COMMANDS = {"upstream": cmd_upstream, "reference": cmd_reference, "trace": cmd_trace, "coreml": cmd_coreml,
            "quantize": cmd_quantize, "contract": cmd_contract, "compile": cmd_compile, "validate": cmd_validate,
            "stage": cmd_stage,
            "publish": cmd_publish}


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("command", choices=list(COMMANDS))
    ap.add_argument("--work", required=True, help="work directory; the upstream download lives in WORK/flex")
    ap.add_argument("--part", choices=PARTS, help="trace / coreml / quantize: which model part")
    ap.add_argument("--revision", default="")
    ap.add_argument("--repo", default=TARGET_REPO)
    ap.add_argument("--mobius-commit", default="")
    ap.add_argument("--simple-voice-ref", default="")
    ap.add_argument("--swap-growth-mb", type=float, default=3072.0,
                    help="abort if swap grows this many MB over the pre-run baseline")
    ap.add_argument("--swap-max-pct", type=float, default=0.0,
                    help="abort if swap passes this %% of the swap total; 0 = off (macOS grows swap on demand)")
    ap.add_argument("--rss-max-gb", type=float, default=12.0, help="abort if the step's RSS passes this")
    ap.add_argument("--disk-floor-gb", type=float, default=8.0,
                    help="abort if free disk on the work volume falls below this")
    ap.add_argument("--keep", action="store_true", help="compile: keep each .mlpackage after its .mlmodelc")
    a = ap.parse_args()
    a.flex = os.path.join(a.work, "flex")
    if a.command in ("trace", "coreml", "quantize") and not a.part:
        die(f"{a.command} needs --part")
    COMMANDS[a.command](a)


if __name__ == "__main__":
    main()
