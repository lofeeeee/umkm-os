#!/usr/bin/env bash
# Agregator CI fail-closed (PRD.md §14.2, BUILD_PLAN.md §2 #8).
# Input: JSON `needs` dari GitHub Actions (env NEEDS_JSON atau argumen 1).
# Lulus HANYA bila daftar job tidak kosong dan SEMUA result == "success".
# skipped / cancelled / failure / result hilang -> gagal.
set -euo pipefail
json=${1:-${NEEDS_JSON:-}}
if [[ -z "$json" ]]; then echo "FAIL: NEEDS_JSON kosong"; exit 1; fi
python3 -I -c '
import json, sys
try:
    needs = json.loads(sys.argv[1])
except ValueError as e:
    print(f"FAIL: NEEDS_JSON bukan JSON valid: {e}"); sys.exit(1)
if not isinstance(needs, dict) or not needs:
    print("FAIL: tidak ada job yang diagregasi"); sys.exit(1)
bad = {}
for name, info in sorted(needs.items()):
    result = info.get("result") if isinstance(info, dict) else None
    print(f"{name}: {result}")
    if result != "success":
        bad[name] = result
if bad:
    print(f"FAIL: {len(bad)} job tidak success: {bad}"); sys.exit(1)
print(f"OK: {len(needs)} job success")
' "$json"
