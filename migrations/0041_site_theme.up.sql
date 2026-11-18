-- Tema situs (brand kit): satu tema untuk semua halaman. Draf diubah dari Studio; ikut terbit saat halaman
-- mana pun diterbitkan, lalu situs publik menimpakan tema terbit ke setiap halaman.
ALTER TABLE tenant_websites
  ADD COLUMN theme_draft jsonb,
  ADD COLUMN theme_published jsonb;
