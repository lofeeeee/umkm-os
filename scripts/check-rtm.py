#!/usr/bin/env python3
"""Penjaga RTM dua arah (artefak #6, BUILD_PLAN.md §7) — fail-closed.

Gagal (exit 1) bila:
  * FR di FSD tidak muncul tepat sekali di RTM §8, atau RTM memuat FR yang tidak ada di FSD;
  * AC di FSD tidak tercakup baris RTM, atau RTM merujuk AC yang tidak ada;
  * AC baris RTM tidak sama dengan AC baris FR di FSD;
  * TC di RTM tidak ada di Test Plan; TC Test Plan menunjuk AC yang tidak ada atau tidak 1:1;
  * status `Verified` tanpa file evidence / lokasi uji yang ada di repo;
  * salah satu dokumen hilang atau tabel kosong (parse gagal = gagal).
"""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
FSD = ROOT / "docs/requirements/02-FSD.md"
TP = ROOT / "docs/test/test-plan.md"
RTM = ROOT / "docs/RTM.md"

errors = []


def read(p):
    if not p.is_file():
        print(f"check-rtm: file hilang: {p.relative_to(ROOT)}")
        sys.exit(1)
    return p.read_text(encoding="utf-8")


def ids(text, kind):
    return re.findall(rf"{kind}-[A-Z]+-\d{{3}}", text)


fsd, tp, rtm = read(FSD), read(TP), read(RTM)

fsd_fr = {}
for fr, ac in re.findall(r"^\| (FR-[A-Z]+-\d{3}) \| .+? \| (.+?) \| .+? \|$", fsd, re.M):
    if fr in fsd_fr:
        errors.append(f"FSD: {fr} didefinisikan dua kali")
    fsd_fr[fr] = set(ids(ac, "AC"))
fsd_ac = set(re.findall(r"^\| (AC-[A-Z]+-\d{3}) \| ", fsd, re.M))

tp_tc = {}
for tc, ac, loc, status in re.findall(
    r"^\| (TC-[A-Z]+-\d{3}) \| (AC-[A-Z]+-\d{3}) \| [^|]+ \| ([^|]+) \| ([^|]+) \|$", tp, re.M
):
    if tc in tp_tc:
        errors.append(f"Test Plan: {tc} dua kali")
    tp_tc[tc] = (ac, loc.strip(), status.strip())

if "## 8." not in rtm:
    errors.append("RTM: bagian §8 tidak ada")
section8 = rtm.split("## 8.", 1)[-1]
rtm_rows = re.findall(
    r"^\| (FR-[A-Z]+-\d{3}) \| ([^|]+) \| ([^|]+) \| [^|]+ \| [^|]+ \| ([^|]+) \| ([^|]+) \|$",
    section8,
    re.M,
)

if not fsd_fr or not fsd_ac or not tp_tc or not rtm_rows:
    errors.append(
        f"parse kosong: FR={len(fsd_fr)} AC={len(fsd_ac)} TC={len(tp_tc)} RTM={len(rtm_rows)}"
    )

seen_fr, covered_ac, rtm_tc = {}, set(), set()
for fr, ac, tc, ev, status in rtm_rows:
    seen_fr[fr] = seen_fr.get(fr, 0) + 1
    acs, tcs = set(ids(ac, "AC")), set(ids(tc, "TC"))
    if fr not in fsd_fr:
        errors.append(f"RTM: {fr} orphan (tidak ada di FSD)")
    elif acs != fsd_fr[fr]:
        errors.append(f"RTM: AC {fr} = {sorted(acs)} != FSD {sorted(fsd_fr[fr])}")
    for a in acs - fsd_ac:
        errors.append(f"RTM: {fr} merujuk {a} yang tidak ada")
    covered_ac |= acs
    rtm_tc |= tcs
    if {t.replace("TC-", "AC-") for t in tcs} != acs:
        errors.append(f"RTM: TC {fr} tidak 1:1 dengan AC")
    if status.strip().startswith("Verified"):
        paths = re.findall(r"`([^`]+)`", ev)
        if not paths or not all((ROOT / p).is_file() for p in paths):
            errors.append(f"RTM: {fr} Verified tanpa evidence yang ada ({ev.strip()})")
        for t in tcs:
            if not tp_tc.get(t, ("", "", ""))[2].startswith("Verified"):
                errors.append(f"RTM: {fr} Verified tetapi {t} belum Verified")

for fr in fsd_fr:
    if seen_fr.get(fr, 0) != 1:
        errors.append(f"RTM: {fr} muncul {seen_fr.get(fr, 0)}x (harus 1)")
for a in fsd_ac - covered_ac:
    errors.append(f"RTM: {a} tidak tercakup")
for t in rtm_tc - set(tp_tc):
    errors.append(f"RTM: {t} tidak ada di Test Plan")
for t, (ac, loc, status) in tp_tc.items():
    if ac not in fsd_ac:
        errors.append(f"Test Plan: {t} -> {ac} tidak ada")
    if t.replace("TC-", "AC-") != ac:
        errors.append(f"Test Plan: {t} harus menguji {t.replace('TC-', 'AC-')}")
    if t not in rtm_tc:
        errors.append(f"Test Plan: {t} orphan (tidak di RTM)")
    if status.startswith("Verified"):
        paths = re.findall(r"`([^`]+)`", loc)
        if not paths or not all((ROOT / p.split("::")[0]).exists() for p in paths):
            errors.append(f"Test Plan: {t} Verified tanpa lokasi uji yang ada ({loc})")

if errors:
    for e in errors:
        print(f"check-rtm: {e}")
    sys.exit(1)
print(f"check-rtm: OK — {len(fsd_fr)} FR, {len(fsd_ac)} AC, {len(tp_tc)} TC")
