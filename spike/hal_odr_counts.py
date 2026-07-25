#!/usr/bin/env python3
"""#41 S4b — structural counts only (no gated content) to sharpen the slice plan.

Reuses the cached decrypted blob. Prints: pass/fail n, whether task_id keys are GAIA UUIDs,
per-task logging-record counts, the message ROLE sequence (roles only), and which ordering
/timing keys exist on a logging record. Everything here is structure/metadata — never the
GAIA question or answer text.
"""
import base64
import json
import os
import re
import zipfile
from collections import Counter

from cryptography.fernet import Fernet
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.kdf.pbkdf2 import PBKDF2HMAC

SCRATCH = "/private/tmp/claude-501/-Users-melvin-Desktop-fantastic-broccoli/a552a415-d253-47d0-af85-0f66195b2799/scratchpad"
ZIP = os.path.join(SCRATCH, "gaia_hf_open_deep_research_gpt4120250414_1744843595_UPLOAD.zip")
UUID_RE = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$")


def load():
    zf = zipfile.ZipFile(ZIP, "r")
    env = json.loads(zf.read(zf.namelist()[0]))
    zf.close()
    salt = base64.b64decode(env["salt"].encode())
    kdf = PBKDF2HMAC(algorithm=hashes.SHA256(), length=32, salt=salt, iterations=480000)
    key = base64.urlsafe_b64encode(kdf.derive(b"hal1234"))
    return json.loads(Fernet(key).decrypt(base64.b64decode(env["encrypted_data"].encode())))


d = load()
res = d["results"]
succ = res.get("successful_tasks", [])
fail = res.get("failed_tasks", [])
print(f"config.agent_name      : {d['config']['agent_name']}")
print(f"config.agent_args.model: {d['config']['agent_args'].get('model_name')}")
print(f"successful_tasks       : {len(succ)}")
print(f"failed_tasks           : {len(fail)}")
print(f"raw_eval_results keys  : {len(d['raw_eval_results'])}")
uuidish = sum(1 for k in d["raw_eval_results"] if UUID_RE.match(str(k)))
print(f"  of which GAIA-UUID    : {uuidish}")
print(f"succ sample is UUID     : {bool(succ) and bool(UUID_RE.match(str(succ[0])))}")

logs = d["raw_logging_results"]
print(f"\nraw_logging_results     : {len(logs)} records")
rec0 = logs[0]
print(f"record top keys         : {list(rec0.keys())}")
# ordering / timing keys anywhere on a record
timing = [k for k in rec0 if any(t in k.lower() for t in ("time", "start", "end", "created", "ts"))]
print(f"record timing-ish keys  : {timing}")
summ = rec0.get("summary", {})
print(f"summary keys            : {list(summ.keys()) if isinstance(summ, dict) else type(summ).__name__}")
if isinstance(summ, dict):
    for sk, sv in summ.items():
        if isinstance(sv, dict):
            print(f"  summary.{sk} keys     : {list(sv.keys())[:12]}")

# per-task record counts via weave_task_id
by_task = Counter()
for r in logs:
    tid = (r.get("attributes") or {}).get("weave_task_id")
    by_task[tid] += 1
counts = sorted(by_task.values())
print(f"\ndistinct weave_task_id  : {len(by_task)}")
if counts:
    print(f"records/task  min/median/max: {counts[0]} / {counts[len(counts)//2]} / {counts[-1]}")

# message ROLE sequence for the first record (roles only, no content)
msgs = ((rec0.get("inputs") or {}).get("messages")) or []
roles = [m.get("role") for m in msgs if isinstance(m, dict)]
print(f"\nrecord0 inputs.messages : {len(msgs)} msgs, roles={roles[:12]}")
out = rec0.get("output")
print(f"record0 output type     : {type(out).__name__}; "
      f"keys={list(out.keys())[:12] if isinstance(out, dict) else '-'}")

# does a passing task's task_id look joinable to TRAIL (same UUID form)?
overlap_form = bool(succ) and all(UUID_RE.match(str(t)) for t in succ[:20])
print(f"\npassing task_ids are GAIA-UUID form (joinable to TRAIL S4a): {overlap_form}")
