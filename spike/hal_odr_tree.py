#!/usr/bin/env python3
"""#41 S4b — full op_name / trace_name + per-task id/parent_id tree (structure only)."""
import base64
import json
import os
import zipfile
from collections import Counter, defaultdict

from cryptography.fernet import Fernet
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.kdf.pbkdf2 import PBKDF2HMAC

SCRATCH = "/private/tmp/claude-501/-Users-melvin-Desktop-fantastic-broccoli/a552a415-d253-47d0-af85-0f66195b2799/scratchpad"
ZIP = os.path.join(SCRATCH, "gaia_hf_open_deep_research_gpt4120250414_1744843595_UPLOAD.zip")


def load():
    zf = zipfile.ZipFile(ZIP, "r")
    env = json.loads(zf.read(zf.namelist()[0]))
    zf.close()
    salt = base64.b64decode(env["salt"].encode())
    kdf = PBKDF2HMAC(algorithm=hashes.SHA256(), length=32, salt=salt, iterations=480000)
    key = base64.urlsafe_b64encode(kdf.derive(b"hal1234"))
    return json.loads(Fernet(key).decrypt(base64.b64decode(env["encrypted_data"].encode())))


d = load()
logs = d["raw_logging_results"]

print("=== full op_name uniqueness (first 5 distinct, raw) ===")
raw_ops = Counter(r.get("op_name") for r in logs)
for name, c in list(raw_ops.items())[:5]:
    print(f"  {c:>5}  {name!r}")
print(f"  distinct raw op_name: {len(raw_ops)}")

print("\n=== summary.weave.trace_name distribution ===")
tn = Counter((r.get("summary", {}).get("weave", {}) or {}).get("trace_name") for r in logs)
for name, c in tn.most_common(10):
    print(f"  {c:>5}  {name!r}")

print("\n=== attributes keys seen (union) ===")
akeys = Counter()
for r in logs:
    for k in (r.get("attributes") or {}):
        akeys[k] += 1
for k, c in akeys.most_common(20):
    print(f"  {c:>5}  {k}")

# sample passing task: reconstruct id/parent_id tree
succ = set(d["results"]["successful_tasks"])
by_task = defaultdict(list)
for r in logs:
    by_task[(r.get("attributes") or {}).get("weave_task_id")].append(r)
sample = next((recs for tid, recs in by_task.items() if tid in succ and 6 < len(recs) < 30), None)
if sample:
    ids = {r["id"] for r in sample}
    children = defaultdict(list)
    roots = []
    for r in sample:
        p = r.get("parent_id")
        (children[p] if (p and p in ids) else roots).append(r)
    order = lambda r: r.get("started_at") or ""
    def show(r, depth):
        wname = (r.get("summary", {}).get("weave", {}) or {}).get("trace_name")
        msgs = (r.get("inputs") or {}).get("messages") or []
        model = (r.get("inputs") or {}).get("model")
        print(f"    {'  '*depth}- tn={wname!r} model={model!r} msgs={len(msgs)} "
              f"out={list((r.get('output') or {}).keys())[:4]}")
        for c in sorted(children[r["id"]], key=order):
            show(c, depth + 1)
    print(f"\n=== sample PASSING task tree ({len(sample)} records, {len(roots)} roots) ===")
    for r in sorted(roots, key=order)[:6]:
        show(r, 0)
