#!/usr/bin/env bash
# Pastikan job `ci-gate` meng-agregasi SEMUA job lain di workflow CI.
# Job baru yang lupa dimasukkan ke `needs` tidak akan memblokir merge -> exit 1.
set -euo pipefail
wf=${1:-.github/workflows/ci.yml}
python3 -I - "$wf" <<'PY'
import re, sys
text = open(sys.argv[1]).read()
body = text.split("\njobs:\n", 1)[1]
jobs = re.findall(r"^  ([A-Za-z0-9_-]+):\s*$", body, re.M)
m = re.search(r"^  ci-gate:\s*$.*?^    needs:\s*\[([^\]]*)\]", body, re.M | re.S)
if "ci-gate" not in jobs or not m:
    print("FAIL: job ci-gate atau needs-nya tidak ditemukan"); sys.exit(1)
needs = {n.strip() for n in m.group(1).split(",") if n.strip()}
expected = set(jobs) - {"ci-gate"}
missing, unknown = expected - needs, needs - expected
if missing or unknown:
    print(f"FAIL: missing={sorted(missing)} unknown={sorted(unknown)}"); sys.exit(1)
block = re.search(r"^  ci-gate:\s*$(.*?)(?=^  \S|\Z)", body, re.M | re.S).group(1)
if not re.search(r"^    if: always\(\)\s*$", block, re.M):
    print("FAIL: ci-gate harus 'if: always()'"); sys.exit(1)
print(f"OK: ci-gate mengagregasi {len(expected)} job: {sorted(expected)}")
PY
