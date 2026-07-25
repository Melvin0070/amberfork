#!/usr/bin/env python3
"""#41 S4b feasibility spike — HF Open Deep Research GAIA reference trace access + format.

Throwaway (spike/, CLAUDE.md permits Python here). Goal from notebook 046: verify ONE HF ODR
GAIA zip's access + on-disk format before scoping the Rust HAL-reference adapter — WITHOUT
downloading the whole 300 MB archive. Strategy: the zip central directory sits at the file
tail, and HF `resolve` URLs honor HTTP Range, so we range-read the EOCD + central directory to
get the full member manifest, then range-read + inflate a single trajectory member to inspect
its JSON shape. Findings get ported to Rust / the notebook, never imported.
"""
import json
import struct
import subprocess
import zlib

BASE = "https://huggingface.co/datasets/agent-evals/hal_traces/resolve/main"
# gpt-4.1 ODR: smallest same-agent reference zip (290 MB) — model is irrelevant to *format*.
ZIP = "gaia_hf_open_deep_research_gpt4120250414_1744843595_UPLOAD.zip"
URL = f"{BASE}/{ZIP}?download=true"

EOCD_SIG = b"PK\x05\x06"
CEN_SIG = 0x02014B50


def http_range(url: str, start: int, end: int) -> bytes:
    # curl handles this env's TLS/CA (urllib in the python.org 3.14 build does not).
    out = subprocess.run(
        ["curl", "-sSL", "--fail", "--max-time", "120", "-r", f"{start}-{end}", url],
        capture_output=True, check=True,
    )
    return out.stdout


def content_length(url: str) -> int:
    out = subprocess.run(
        ["curl", "-sSL", "-I", "--max-time", "60", url],
        capture_output=True, check=True, text=True,
    )
    # HF `resolve` redirects to a CDN for LFS blobs; take the LAST content-length in the
    # redirect chain (the real object), not the pointer stub's.
    sizes = [
        int(line.split(":", 1)[1].strip())
        for line in out.stdout.splitlines()
        if line.lower().startswith("content-length:")
    ]
    if not sizes:
        raise RuntimeError("no Content-Length header")
    return sizes[-1]


def parse_central_directory(cd: bytes):
    """Yield (name, method, comp_size, uncomp_size, local_header_off) per central entry."""
    off = 0
    while off + 4 <= len(cd) and struct.unpack_from("<I", cd, off)[0] == CEN_SIG:
        (method,) = struct.unpack_from("<H", cd, off + 10)
        comp, uncomp = struct.unpack_from("<II", cd, off + 20)
        n, m, k = struct.unpack_from("<HHH", cd, off + 28)
        (lho,) = struct.unpack_from("<I", cd, off + 42)
        name = cd[off + 46 : off + 46 + n].decode("utf-8", "replace")
        yield name, method, comp, uncomp, lho
        off += 46 + n + m + k


def fetch_member(url: str, lho: int, method: int, comp: int) -> bytes:
    # Local header: 30 fixed bytes + name + extra, then the compressed data.
    head = http_range(url, lho, lho + 29)
    n, m = struct.unpack_from("<HH", head, 26)
    data_start = lho + 30 + n + m
    raw = http_range(url, data_start, data_start + comp - 1)
    if method == 0:
        return raw
    return zlib.decompress(raw, -15)  # raw deflate


def main():
    size = content_length(URL)
    print(f"zip: {ZIP}\nsize: {size:,} bytes ({size / 1e6:.0f} MB)\n")

    tail = http_range(URL, size - 1_500_000, size - 1)
    eocd = tail.rfind(EOCD_SIG)
    if eocd < 0:
        print("no EOCD in tail (zip64?) — need a bigger tail read")
        return
    total, cd_size, cd_off = (
        struct.unpack_from("<H", tail, eocd + 10)[0],
        struct.unpack_from("<I", tail, eocd + 12)[0],
        struct.unpack_from("<I", tail, eocd + 16)[0],
    )
    print(f"central directory: {total} entries, {cd_size:,} bytes at offset {cd_off:,}")

    # Grab the central directory (range-read if it wasn't already in the tail).
    if cd_off >= size - 1_500_000:
        cd = tail[cd_off - (size - 1_500_000) : cd_off - (size - 1_500_000) + cd_size]
    else:
        cd = http_range(URL, cd_off, cd_off + cd_size - 1)

    entries = list(parse_central_directory(cd))
    print(f"parsed {len(entries)} entries\n--- top-level layout (first path component) ---")
    tops = {}
    for name, *_ in entries:
        top = name.split("/")[0] if "/" in name else "(root)"
        tops[top] = tops.get(top, 0) + 1
    for t, c in sorted(tops.items()):
        print(f"  {t}/  ({c} entries)")

    print("\n--- sample of member names + uncompressed sizes ---")
    for name, method, comp, uncomp, _ in entries[:25]:
        print(f"  {uncomp:>12,}  m{method}  {name}")

    # Find the smallest plausible JSON trajectory member and inspect its shape.
    jsons = [e for e in entries if e[0].lower().endswith(".json") and e[3] > 0]
    jsons.sort(key=lambda e: e[3])
    if not jsons:
        print("\nno .json members found — format is not per-file JSON")
        return
    name, method, comp, uncomp, lho = jsons[len(jsons) // 2]  # median-size json
    print(f"\n--- inflating a representative JSON member ---\n  {name} ({uncomp:,} B)")
    body = fetch_member(URL, lho, method, comp)
    try:
        obj = json.loads(body)
    except Exception as e:  # noqa: BLE001
        print(f"  not valid JSON: {e}; first 300 bytes:\n{body[:300]!r}")
        return
    print(f"  top-level type: {type(obj).__name__}")
    if isinstance(obj, dict):
        print(f"  keys: {list(obj.keys())[:40]}")
    elif isinstance(obj, list):
        print(f"  length: {len(obj)}; item0 keys: "
              f"{list(obj[0].keys())[:40] if obj and isinstance(obj[0], dict) else '?'}")


if __name__ == "__main__":
    main()
