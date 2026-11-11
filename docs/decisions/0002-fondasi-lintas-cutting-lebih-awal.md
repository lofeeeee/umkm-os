---
status: Accepted
date: 2026-11-11
decision-makers: Product owner (pemilik repo)
---

# ADR-0002 — Kapabilitas lintas-cutting dibangun di Phase 0–1

## Context and Problem Statement

`PRD.md §15` menempatkan audit, notifikasi/outbox, dan sebagian observability di fase belakangan, padahal DoD `PRD.md §16` butir 7/9/11/13/14 berlaku untuk setiap fitur sejak fitur pertama (`BUILD_PLAN.md §1`).

## Considered Options

* Bangun audit, outbox, upload, OpenAPI, observability di Phase 0–1
* Ikuti urutan `PRD.md §15` apa adanya

## Decision Outcome

Chosen option: "Bangun di Phase 0–1", because tanpa itu fitur pertama tidak bisa memenuhi DoD `PRD.md §16`.

### Consequences

* Good, because setiap fitur Phase 2+ langsung punya audit, outbox, metrics, dan kontrak OpenAPI.
* Bad, because Phase 0–1 lebih panjang sebelum ada fitur yang terlihat pengguna.

## More Information

Status keputusan di `OPEN_DECISIONS.md`: `DISETUJUI` 2026-11-11. Tidak mengubah requirement `PRD.md`; hanya urutan kerja.
