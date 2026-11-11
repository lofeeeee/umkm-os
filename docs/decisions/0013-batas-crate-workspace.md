---
status: Accepted
date: 2026-11-11
decision-makers: Product owner (pemilik repo)
---

# ADR-0013 — Batas crate workspace: 5 crate inti + 2 app

## Context and Problem Statement

`PRD.md §7.3` menyarankan 20 crate + 2 app sebagai starting point, sekaligus melarang crate terlalu kecil tanpa boundary nyata. `PRD.md §7.2` mewajibkan domain tidak bergantung pada Axum atau implementasi database.

## Considered Options

* 5 crate inti (`shared`, `domain`, `application`, `infrastructure`, `database`) + `apps/api`, `apps/worker`
* 20 crate penuh `PRD.md §7.3`
* Satu crate core

## Decision Outcome

Chosen option: "5 crate inti", because lapisan `PRD.md §7.2` dijaga compiler tanpa membuat 15 crate kosong.

Arah dependensi yang diizinkan:

```text
apps/api, apps/worker -> application, infrastructure, database, shared
application           -> domain, shared
infrastructure        -> domain, shared, database
database              -> shared
domain                -> shared            (tanpa axum / sqlx / reqwest / tokio)
```

Modul feature (`auth`, `tenant`, `rbac`, `catalog`, …) dimulai sebagai modul di dalam `domain`/`application`. Pemisahan menjadi crate tersendiri membutuhkan ADR baru yang menautkan ADR ini.

### Consequences

* Good, because batas lapisan dipaksa compiler sejak P0-1; kompilasi tetap cepat.
* Bad, because pemisahan modul feature ke crate nanti butuh refactor + ADR.

## More Information

Status keputusan di `OPEN_DECISIONS.md`: `DISETUJUI` 2026-11-11. Boundary check `domain` dijaga CI di P0-7.
