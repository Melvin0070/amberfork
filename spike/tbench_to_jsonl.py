#!/usr/bin/env python3
"""Terminal-Bench 2.0 trajectory parquet -> JSON-Lines, one trial per line (notebook 083).

The corpus ships as parquet; the Rust harness reads JSON-Lines so the workspace takes no parquet
dependency. Run with an ephemeral environment — nothing is installed into the project:

    uv run --no-project --with pyarrow python -I spike/tbench_to_jsonl.py \
        bench/data/tbench/train-*.parquet > bench/data/tbench/trials.jsonl

Rows are passed through unchanged (`steps` stays the JSON string it ships as); only the columns
the harness reads are kept. Benchmark-use only: the output belongs under bench/data/ (gitignored).
"""

import json
import sys

import pyarrow.parquet as pq

COLUMNS = ["task_name", "agent", "model", "reward", "trial_name", "trial_id", "steps"]


def main(paths):
    for path in paths:
        for batch in pq.ParquetFile(path).iter_batches(batch_size=500, columns=COLUMNS):
            for row in batch.to_pylist():
                sys.stdout.write(json.dumps(row, ensure_ascii=False))
                sys.stdout.write("\n")


if __name__ == "__main__":
    main(sys.argv[1:])
