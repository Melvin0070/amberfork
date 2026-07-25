#!/usr/bin/env python3
"""#41 S4b — op_name distribution + tree linkage (metadata only) to design the Weave->Step map.

No gated content: op_names are operation/function identifiers, counts, and structural flags.
Reuses the cached decrypted blob.
"""
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

# op_name shape: strip the trailing content hash Weave appends after ':' if any.
def base_op(name):
    return name.split(":")[0] if isinstance(name, str) else str(name)

ops = Counter(base_op(r.get("op_name")) for r in logs)
print(f"=== op_name base distribution ({len(ops)} distinct) ===")
for name, c in ops.most_common(30):
    print(f"  {c:>5}  {name}")

# per-task tree linkage: root = record whose parent_id is null or points outside the task's ids
succ = set(d["results"]["successful_tasks"])
by_task = defaultdict(list)
for r in logs:
    tid = (r.get("attributes") or {}).get("weave_task_id")
    by_task[tid].append(r)

roots_hist = Counter()
missing_parent = 0
sample_task = None
for tid, recs in by_task.items():
    ids = {r["id"] for r in recs}
    roots = [r for r in recs if not r.get("parent_id") or r["parent_id"] not in ids]
    roots_hist[len(roots)] += 1
    if tid in succ and sample_task is None and len(recs) > 5:
        sample_task = (tid, recs, roots)

print(f"\n=== per-task root count histogram (roots -> #tasks) ===")
for n, c in sorted(roots_hist.items()):
    print(f"  {n} root(s): {c} tasks")

# structural flags across all records
has_input = sum(1 for r in logs if (r.get("inputs") or {}).get("messages") is not None)
has_output = sum(1 for r in logs if r.get("output") is not None)
has_exc = sum(1 for r in logs if r.get("exception"))
has_tid = sum(1 for r in logs if (r.get("attributes") or {}).get("weave_task_id"))
print(f"\n=== record structural flags (of {len(logs)}) ===")
print(f"  inputs.messages present : {has_input}")
print(f"  output present          : {has_output}")
print(f"  exception present       : {has_exc}")
print(f"  weave_task_id present   : {has_tid}")

# one passing task: op_name sequence in started_at order (structure only)
if sample_task:
    tid, recs, roots = sample_task
    recs_sorted = sorted(recs, key=lambda r: r.get("started_at") or "")
    print(f"\n=== sample PASSING task {tid}: {len(recs)} records, {len(roots)} root(s) ===")
    print("  op_name sequence (started_at order):")
    for r in recs_sorted[:20]:
        op = base_op(r.get("op_name"))
        exc = " [EXC]" if r.get("exception") else ""
        inp = "msgs" if (r.get("inputs") or {}).get("messages") is not None else "-"
        print(f"    {op}{exc}  (inputs:{inp})")
    print(f"  display_name sample: {recs_sorted[0].get('display_name')!r}")
