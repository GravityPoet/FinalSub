#!/usr/bin/env python3
"""Offline Parakeet TDT 0.6B v2 MLX transcription helper for FinalSub."""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path


MIN_MODEL_BYTES = 1_000_000_000


def normalize_spaces(text: str) -> str:
    return re.sub(r"\s+", " ", text).strip()


def format_srt_time(seconds: float) -> str:
    seconds = max(0.0, seconds)
    total_ms = int(round(seconds * 1000))
    hours, remainder = divmod(total_ms, 3_600_000)
    minutes, remainder = divmod(remainder, 60_000)
    secs, milliseconds = divmod(remainder, 1000)
    return f"{hours:02d}:{minutes:02d}:{secs:02d},{milliseconds:03d}"


def write_srt(blocks: list[dict[str, float | str]], output_path: Path) -> None:
    lines: list[str] = []
    for index, block in enumerate(blocks, 1):
        lines.extend(
            [
                str(index),
                f"{format_srt_time(float(block['start']))} --> {format_srt_time(float(block['end']))}",
                str(block["text"]),
                "",
            ]
        )

    output_path.parent.mkdir(parents=True, exist_ok=True)
    temporary_path = output_path.with_name(f".{output_path.name}.tmp")
    temporary_path.write_text("\n".join(lines), encoding="utf-8")
    os.replace(temporary_path, output_path)


def validate_local_model(model_path: Path) -> Path:
    resolved = model_path.expanduser().resolve(strict=True)
    config_path = resolved / "config.json"
    weights_path = resolved / "model.safetensors"
    if not config_path.is_file():
        raise ValueError(f"Parakeet config.json is missing: {config_path}")
    if not weights_path.is_file() or weights_path.stat().st_size < MIN_MODEL_BYTES:
        raise ValueError(f"Parakeet model.safetensors is missing or incomplete: {weights_path}")
    return resolved


def transcribe(
    audio_path: Path,
    output_path: Path,
    local_model_path: Path,
    source_language: str,
    chunk_duration: float,
    overlap_duration: float,
    max_line_ms: int,
    pause_ms: int,
    max_block_chars: int,
) -> None:
    if source_language.lower() not in ("auto", "en", "english"):
        raise ValueError(
            f"Parakeet v2 only supports English transcription, got: {source_language}"
        )
    if not audio_path.is_file():
        raise ValueError(f"Audio file is missing: {audio_path}")

    model_path = validate_local_model(local_model_path)

    try:
        from parakeet_mlx import DecodingConfig, SentenceConfig, from_pretrained
    except ModuleNotFoundError as error:
        raise RuntimeError("parakeet-mlx is not installed in the uv runtime") from error

    max_words = max(8, min(28, max_block_chars // 5))
    sentence_config = SentenceConfig(
        max_words=max_words,
        silence_gap=max(0.2, pause_ms / 1000),
        max_duration=max(1.0, max_line_ms / 1000),
    )
    decoding_config = DecodingConfig(sentence=sentence_config)

    print(json.dumps({"status": "loading_model", "runtime": "mlx"}), flush=True)
    # Passing the resolved snapshot directory prevents any Hugging Face network lookup.
    model = from_pretrained(str(model_path))

    print(json.dumps({"status": "transcribing", "runtime": "mlx"}), flush=True)
    result = model.transcribe(
        str(audio_path),
        decoding_config=decoding_config,
        chunk_duration=chunk_duration,
        overlap_duration=overlap_duration,
    )

    blocks: list[dict[str, float | str]] = []
    for sentence in getattr(result, "sentences", []) or []:
        text = normalize_spaces(str(getattr(sentence, "text", "")))
        if not text:
            continue
        start = float(getattr(sentence, "start"))
        end = float(getattr(sentence, "end"))
        if end <= start:
            end = start + 0.3
        if blocks:
            start = max(start, float(blocks[-1]["end"]))
        if end <= start:
            end = start + 0.3
        blocks.append({"start": start, "end": end, "text": text})

    write_srt(blocks, output_path)
    print(
        json.dumps(
            {"status": "done", "runtime": "mlx", "sentence_count": len(blocks)}
        ),
        flush=True,
    )


def main() -> None:
    parser = argparse.ArgumentParser(description="FinalSub Parakeet v2 MLX worker")
    parser.add_argument("--audio", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--local-model-path", required=True, type=Path)
    parser.add_argument("--source-language", default="auto")
    parser.add_argument("--chunk-duration", type=float, default=120.0)
    parser.add_argument("--overlap-duration", type=float, default=15.0)
    parser.add_argument("--max-line-ms", type=int, default=6000)
    parser.add_argument("--pause-ms", type=int, default=500)
    parser.add_argument("--max-block-chars", type=int, default=84)
    args = parser.parse_args()

    try:
        transcribe(
            audio_path=args.audio,
            output_path=args.output,
            local_model_path=args.local_model_path,
            source_language=args.source_language,
            chunk_duration=args.chunk_duration,
            overlap_duration=args.overlap_duration,
            max_line_ms=args.max_line_ms,
            pause_ms=args.pause_ms,
            max_block_chars=args.max_block_chars,
        )
    except Exception as error:
        print(json.dumps({"error": str(error)}), file=sys.stderr, flush=True)
        raise


if __name__ == "__main__":
    main()
