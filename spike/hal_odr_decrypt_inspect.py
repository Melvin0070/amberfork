#!/usr/bin/env python3
"""#41 S4b feasibility spike, part 2 — decrypt ONE HF ODR GAIA zip + inspect the inner schema.

Recipe is HAL's own `static/downloads/hal-decrypt.sh` (fetched this session), not guessed:
  zip -> single `.json.encrypted` member -> JSON envelope {salt, encrypted_data}
  key  = urlsafe_b64( PBKDF2HMAC(SHA256, len=32, salt=b64d(salt), iters=480000).derive(b"hal1234") )
  json = Fernet(key).decrypt( b64d(encrypted_data) )
The decrypted JSON is one blob for the whole agent config (all GAIA tasks). We summarize its
STRUCTURE only (never dump content — GAIA lineage is gated): what scopes the Rust adapter is how
runs are grouped per task_id and where the pass/fail + trajectory live. Throwaway; port to Rust.
"""
import base64
import json
import os
import subprocess
import sys
import zipfile

from cryptography.fernet import Fernet
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.kdf.pbkdf2 import PBKDF2HMAC

BASE = "https://huggingface.co/datasets/agent-evals/hal_traces/resolve/main"
ZIP = "gaia_hf_open_deep_research_gpt4120250414_1744843595_UPLOAD.zip"
SCRATCH = "/private/tmp/claude-501/-Users-melvin-Desktop-fantastic-broccoli/a552a415-d253-47d0-af85-0f66195b2799/scratchpad"


def download(url: str, dest: str) -> None:
    if os.path.exists(dest) and os.path.getsize(dest) > 1_000_000:
        print(f"[cache] {dest} ({os.path.getsize(dest)/1e6:.0f} MB)")
        return
    print(f"[download] {url}")
    subprocess.run(["curl", "-sSL", "--fail", "--max-time", "600", "-o", dest, url], check=True)
    print(f"[download] done, {os.path.getsize(dest)/1e6:.0f} MB")


def decrypt(zip_path: str) -> object:
    zf = zipfile.ZipFile(zip_path, "r")
    name = zf.namelist()[0]
    print(f"[decrypt] member: {name} ({zf.getinfo(name).file_size/1e6:.0f} MB inflated)")
    envelope = json.loads(zf.read(name))
    zf.close()
    print(f"[decrypt] envelope keys: {list(envelope.keys())}")
    salt = base64.b64decode(envelope["salt"].encode())
    kdf = PBKDF2HMAC(algorithm=hashes.SHA256(), length=32, salt=salt, iterations=480000)
    key = base64.urlsafe_b64encode(kdf.derive(b"hal1234"))
    ct = base64.b64decode(envelope["encrypted_data"].encode())
    plaintext = Fernet(key).decrypt(ct)
    print(f"[decrypt] plaintext {len(plaintext)/1e6:.0f} MB — parsing JSON")
    return json.loads(plaintext)


def shape(obj, depth=0, max_depth=3):
    pad = "  " * depth
    if isinstance(obj, dict):
        keys = list(obj.keys())
        print(f"{pad}dict[{len(keys)}] keys: {keys[:20]}")
        if depth < max_depth and keys:
            k = keys[0]
            print(f"{pad}└ sample value for {k!r}:")
            shape(obj[k], depth + 1, max_depth)
    elif isinstance(obj, list):
        print(f"{pad}list[{len(obj)}]")
        if depth < max_depth and obj:
            shape(obj[0], depth + 1, max_depth)
    else:
        s = repr(obj)
        print(f"{pad}{type(obj).__name__}: {s[:80]}")


def main():
    zip_path = os.path.join(SCRATCH, ZIP)
    download(f"{BASE}/{ZIP}?download=true", zip_path)
    data = decrypt(zip_path)
    print("\n================ TOP-LEVEL STRUCTURE ================")
    shape(data, max_depth=4)

    # Hunt for the signals the adapter needs: task_id grouping, pass/fail, trajectory/steps.
    print("\n================ SIGNAL HUNT ================")
    text_probe_keys = ("task_id", "task", "gaia", "correct", "score", "pass", "success",
                       "steps", "trace", "messages", "trajectory", "result", "raw_logging",
                       "model", "agent", "results", "total")
    def find_keys(obj, path="$", hits=None, seen=0):
        if hits is None:
            hits = {}
        if seen > 4000:
            return hits
        if isinstance(obj, dict):
            for k, v in obj.items():
                lk = str(k).lower()
                for probe in text_probe_keys:
                    if probe in lk and k not in hits:
                        hits[k] = f"{path}.{k}  ({type(v).__name__})"
                find_keys(v, f"{path}.{k}", hits, seen + 1)
        elif isinstance(obj, list) and obj:
            find_keys(obj[0], f"{path}[0]", hits, seen + 1)
        return hits
    for k, where in sorted(find_keys(data).items()):
        print(f"  {where}")


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as e:
        print(f"download failed: {e}", file=sys.stderr)
        sys.exit(1)
