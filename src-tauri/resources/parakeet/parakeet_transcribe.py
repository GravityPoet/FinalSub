#!/usr/bin/env python3
"""Parakeet TDT 0.6B v2 transcription helper for FinalSub."""

import argparse
import json
import sys
import os
from pathlib import Path


def normalize_spaces(text: str) -> str:
    import re
    text = re.sub(r"\s+", " ", text).strip()
    return text


def format_srt_time(seconds: float) -> str:
    if seconds < 0:
        seconds = 0.0
    h = int(seconds // 3600)
    m = int((seconds % 3600) // 60)
    s = int(seconds % 60)
    ms = int(round((seconds - int(seconds)) * 1000))
    if ms >= 1000:
        ms = 999
    return f"{h:02d}:{m:02d}:{s:02d},{ms:03d}"


def write_srt(blocks: list[dict], output_path: Path) -> None:
    lines = []
    for i, block in enumerate(blocks, 1):
        start = format_srt_time(block["start"])
        end = format_srt_time(block["end"])
        text = block["text"]
        lines.append(f"{i}")
        lines.append(f"{start} --> {end}")
        lines.append(text)
        lines.append("")
    output_path.write_text("\n".join(lines), encoding="utf-8")


def transcribe(
    audio_path: str,
    output_path: str,
    model_name: str,
    cache_root: str,
    source_language: str,
    chunk_duration: float = 120.0,
    overlap_duration: float = 15.0,
    max_line_ms: int = 6000,
    pause_ms: int = 500,
    max_block_chars: int = 84,
) -> None:
    if source_language and source_language.lower() not in ("auto", "en", "english"):
        print(
            json.dumps(
                {
                    "error": f"Parakeet v2 only supports English transcription in this workflow, got: {source_language}"
                }
            ),
            file=sys.stderr,
        )
        sys.exit(1)

    try:
        from parakeet_mlx import DecodingConfig, SentenceConfig, from_pretrained
    except ModuleNotFoundError:
        print(
            json.dumps({"error": "parakeet-mlx is not installed. Run: uv pip install parakeet-mlx"}),
            file=sys.stderr,
        )
        sys.exit(1)

    cache_dir = Path(cache_root) / "parakeet-models" / "huggingface"
    cache_dir.mkdir(parents=True, exist_ok=True)

    max_words = max(8, min(28, max_block_chars // 5))
    sentence_config = SentenceConfig(
        max_words=max_words,
        silence_gap=max(0.2, pause_ms / 1000),
        max_duration=max(1.0, max_line_ms / 1000),
    )
    decoding_config = DecodingConfig(sentence=sentence_config)

    print(json.dumps({"status": "loading_model", "model": model_name}), flush=True)

    model = from_pretrained(model_name, cache_dir=str(cache_dir))

    print(json.dumps({"status": "transcribing", "audio": audio_path}), flush=True)

    result = model.transcribe(
        audio_path,
        decoding_config=decoding_config,
        chunk_duration=chunk_duration,
        overlap_duration=overlap_duration,
    )

    blocks = []
    for sentence in getattr(result, "sentences", []) or []:
        text = normalize_spaces(str(getattr(sentence, "text", "")))
        if not text:
            continue
        start = float(getattr(sentence, "start"))
        end = float(getattr(sentence, "end"))
        if end <= start:
            end = start + 0.3
        if blocks and start < blocks[-1]["end"]:
            start = blocks[-1]["end"]
        if end <= start:
            end = start + 0.3
        blocks.append({"start": start, "end": end, "text": text})

    write_srt(blocks, Path(output_path))

    print(
        json.dumps(
            {
                "status": "done",
                "output": output_path,
                "sentence_count": len(blocks),
                "model": model_name,
            }
        ),
        flush=True,
    )


def main() -> None:
    parser = argparse.ArgumentParser(description="Parakeet v2 transcription for FinalSub")
    parser.add_argument("--audio", required=True, help="Path to input audio file (WAV, 16kHz mono)")
    parser.add_argument("--output", required=True, help="Path to output SRT file")
    parser.add_argument("--model", default="mlx-community/parakeet-tdt-0.6b-v2", help="HuggingFace model name")
    parser.add_argument("--cache-root", default=os.path.expanduser("~/Tools/Local-LLM"), help="Model cache root directory")
    parser.add_argument("--source-language", default="auto", help="Source language (auto or en)")
    parser.add_argument("--chunk-duration", type=float, default=120.0, help="Chunk duration in seconds")
    parser.add_argument("--overlap-duration", type=float, default=15.0, help="Overlap duration in seconds")
    parser.add_argument("--max-line-ms", type=int, default=6000, help="Max line duration in ms")
    parser.add_argument("--pause-ms", type=int, default=500, help="Pause duration in ms")
    parser.add_argument("--max-block-chars", type=int, default=84, help="Max block characters")
    args = parser.parse_args()

    transcribe(
        audio_path=args.audio,
        output_path=args.output,
        model_name=args.model,
        cache_root=args.cache_root,
        source_language=args.source_language,
        chunk_duration=args.chunk_duration,
        overlap_duration=args.overlap_duration,
        max_line_ms=args.max_line_ms,
        pause_ms=args.pause_ms,
        max_block_chars=args.max_block_chars,
    )


if __name__ == "__main__":
    main()
