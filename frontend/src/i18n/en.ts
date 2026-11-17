// Kamus English. Kunci = teks Indonesia persis seperti di kode (lihat src/i18n/index.ts).
// Dijaga lengkap oleh src/i18n/catalog.test.ts. Istilah baku: docs/ux/pedoman-label.md (Glosarium ID ↔ EN).
import { APP_3 } from "./en/app-3";
import { APP_4 } from "./en/app-4";
import { DASHBOARD_1 } from "./en/dashboard-1";
import { DASHBOARD_2 } from "./en/dashboard-2";
import { TEMPLATES_5 } from "./en/templates-5";

export const EN_PARTS = { DASHBOARD_1, DASHBOARD_2, APP_3, APP_4, TEMPLATES_5 };
export const EN: Record<string, string> = { ...DASHBOARD_1, ...DASHBOARD_2, ...APP_3, ...APP_4, ...TEMPLATES_5 };
