import { t } from "@/i18n";
// Siapkan foto sebelum diunggah: API menolak body > ±2 MB (server.body_limit_bytes) dan memutus koneksi
// (browser hanya melihat ERR_CONNECTION_ABORTED). Foto HP biasanya 3–8 MB, jadi dikecilkan di sini:
// sisi terpanjang ≤ 1920 px, WebP (fallback JPEG), kualitas diturunkan bertahap sampai < 1,8 MB.
// PNG kecil / file yang sudah cukup kecil dikirim apa adanya; PNG/WebP besar tetap transparan (WebP/PNG, bukan JPEG).

export const MAX_UPLOAD_BYTES = 1_800_000;
const MAX_SIDE = 1920;
const ACCEPTED = ["image/png", "image/jpeg", "image/webp"];
const EXT: Record<string, string> = { "image/webp": "webp", "image/png": "png", "image/jpeg": "jpg" };

export class ImagePrepError extends Error {}

function load(file: File): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(file);
    const img = new Image();
    img.onload = () => { URL.revokeObjectURL(url); resolve(img); };
    img.onerror = () => { URL.revokeObjectURL(url); reject(new ImagePrepError(t("Format foto tidak bisa dibaca browser. Pakai JPEG, PNG, atau WebP."))); };
    img.src = url;
  });
}

const toBlob = (c: HTMLCanvasElement, type: string, q: number) =>
  new Promise<Blob | null>((resolve) => c.toBlob(resolve, type, q));

export async function prepareImage(file: File): Promise<File> {
  if (!file.type.startsWith("image/")) throw new ImagePrepError(t("Pilih file gambar (JPEG, PNG, atau WebP)."));
  if (ACCEPTED.includes(file.type) && file.size <= MAX_UPLOAD_BYTES) return file;

  const img = await load(file);
  const ratio = Math.min(1, MAX_SIDE / Math.max(img.naturalWidth, img.naturalHeight));
  let w = Math.max(1, Math.round(img.naturalWidth * ratio));
  let h = Math.max(1, Math.round(img.naturalHeight * ratio));
  // PNG/WebP bisa transparan (stiker, logo, potongan objek): jangan pernah jadi JPEG yang meratakan
  // transparansi ke hitam/putih — WebP (alfa tetap) lalu PNG.
  const types = file.type === "image/png" || file.type === "image/webp" ? ["image/webp", "image/png"] : ["image/webp", "image/jpeg"];
  const canvas = document.createElement("canvas");
  for (let attempt = 0; attempt < 6; attempt++) {
    canvas.width = w;
    canvas.height = h;
    const ctx = canvas.getContext("2d");
    if (!ctx) break;
    ctx.drawImage(img, 0, 0, w, h);
    for (const q of [0.86, 0.78, 0.7, 0.6]) {
      for (const type of types) {
        // Browser yang tidak bisa meng-encode `type` mengembalikan PNG — hanya terima jenis yang diminta.
        const blob = await toBlob(canvas, type, q);
        if (blob && blob.type === type && blob.size <= MAX_UPLOAD_BYTES) {
          return new File([blob], file.name.replace(/\.[^.]+$/, "") + `.${EXT[type]}`, { type });
        }
      }
    }
    // Masih terlalu besar: perkecil dimensi lalu coba lagi.
    w = Math.round(w * 0.75);
    h = Math.round(h * 0.75);
  }
  throw new ImagePrepError(t("Foto terlalu besar meski sudah dikompres. Coba foto lain."));
}
