---
status: Accepted
date: 2026-11-12
decision-makers: Product owner (pemilik repo)
---

# ADR-0006 — Channel messaging: web chat dulu, WhatsApp/Telegram lewat adapter menyusul

## Context and Problem Statement

D-04 (`PRD.md §20 #4`, `PRD.md §4.12`, `PRD.md §7.5`): customer service memakai web chat, WhatsApp, atau keduanya.
WhatsApp/Telegram membutuhkan provider messaging, opt-in/consent channel, dan webhook masuk. Pemilik produk memutuskan
(2026-11-12): **"Whatsapp/telegram later"**.

## Considered Options

* Web chat saja di V1, channel lain lewat adapter menyusul (rekomendasi engineering)
* WhatsApp sejak awal
* Web chat + WhatsApp + Telegram sejak awal

## Decision Outcome

Chosen option: "Web chat saja di V1, WhatsApp/Telegram lewat adapter menyusul".

| Aturan | Nilai |
|---|---|
| Channel V1 | widget web chat di storefront tenant (`{slug}.ryasai.my.id`), pesan masuk lewat HTTP, pesan keluar lewat SSE |
| Model data | `conversations.channel` berupa enum dengan nilai awal `web`; channel baru = nilai enum baru + adapter, tanpa mengubah skema pesan |
| Port | domain/application tidak tahu transport; pengiriman balasan admin/AI lewat event yang dibaca adapter channel |
| Identitas | sesi chat anonim (token sesi acak per percakapan, hash disimpan) atau customer terautentikasi (D-03); order hanya terbaca untuk customer terverifikasi |
| Consent | teks consent AI (D-12, placeholder) ditampilkan sebelum pesan pertama; versi + waktu disimpan per percakapan |
| WhatsApp/Telegram | di luar V1; menunggu ADR baru (provider, opt-in, verifikasi signature webhook, template message) |

### Consequences

* Good: tidak ada provider/biaya messaging pihak ketiga di V1; seluruh jalur bisa diuji end-to-end lokal.
* Bad: pelanggan yang terbiasa WhatsApp harus membuka website toko.

## More Information

Status di `OPEN_DECISIONS.md`: D-04 `DISETUJUI` 2026-11-12. Append-only.
