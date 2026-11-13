// Teks legal UMKM Business OS (D-12). SATU sumber untuk dashboard (/legal/*) dan situs toko
// (/kebijakan-privasi, /syarat-transaksi). Versi harus sama dengan config `[legal]` di server
// (`privacy_policy_version` / `terms_of_service_version`) karena versi itulah yang tercatat saat persetujuan.
//
// Disusun sendiri oleh tim (keputusan pemilik 2026-11-13: "kita juga bagian dari UMKM mandiri"), BELUM ditinjau
// konsultan hukum. Mengubah isi = naikkan versi di sini DAN di config, dan catat di product-decision-log.

export const LEGAL_VERSION = "v1-20261113";
export const LEGAL_EFFECTIVE = "13 November 2026";
/** Alamat kontak resmi untuk pertanyaan & permintaan data. Wajib diisi sebelum go-live. */
export const LEGAL_CONTACT = "[alamat email kontak resmi — diisi sebelum peluncuran]";

export interface Section { title: string; paragraphs: string[] }

export const PRIVACY: Section[] = [
  {
    title: "1. Tentang kebijakan ini",
    paragraphs: [
      "UMKM Business OS (\"kami\") adalah layanan yang membantu usaha mikro, kecil, dan menengah membuat website toko, menerima pesanan, melayani pelanggan, dan memakai asisten AI. Kebijakan ini menjelaskan data apa yang kami kumpulkan, untuk apa, berapa lama disimpan, dan hak Anda.",
      "Ada dua peran: (a) untuk akun pemilik toko dan anggota timnya, kami adalah pihak yang mengelola data; (b) untuk data pembeli di toko, pemilik toko adalah pihak yang menentukan penggunaan data, sedangkan kami memproses data tersebut atas nama toko agar layanan berjalan.",
    ],
  },
  {
    title: "2. Data yang kami proses",
    paragraphs: [
      "Akun pemilik toko & tim: nama usaha, alamat email, password (disimpan dalam bentuk hash, tidak dapat dibaca), pengaturan keamanan seperti verifikasi dua langkah, peran dan izin, serta catatan aktivitas penting (audit log).",
      "Data toko: profil usaha, produk, stok, pesanan, tiket, file gambar yang diunggah, dan bukti pembayaran langganan.",
      "Data pembeli (diproses atas nama toko): nama, email atau nomor telepon, alamat pengiriman bila dipilih, isi pesanan, bukti pembayaran, akun pembeli bila dibuat, serta isi percakapan chat dengan toko atau asisten AI toko.",
      "Data teknis: alamat IP dan informasi perangkat untuk keamanan (misalnya pembatasan percobaan login) dan pemantauan layanan. Kami tidak memakai cookie iklan atau pelacak pihak ketiga.",
    ],
  },
  {
    title: "3. Untuk apa data dipakai",
    paragraphs: [
      "Menjalankan layanan: menampilkan website toko, memproses pesanan dan pembayaran, mengirim notifikasi, menyediakan dukungan pelanggan, dan menagih langganan.",
      "Keamanan: mencegah penyalahgunaan, mendeteksi akses tidak sah, dan mencatat tindakan penting agar dapat ditelusuri.",
      "Kami tidak menjual data pribadi dan tidak memakainya untuk iklan.",
    ],
  },
  {
    title: "4. Asisten AI",
    paragraphs: [
      "Bila toko mengaktifkan asisten AI, pesan pembeli dan informasi toko yang relevan (misalnya katalog dan jam buka) dikirim ke penyedia model AI untuk menghasilkan jawaban. Penyedianya bisa milik platform atau milik toko sendiri (kunci API toko). Jawaban AI dapat keliru; pembeli dapat meminta bicara dengan staf kapan saja.",
      "Pembeli diberi tahu dan diminta persetujuan sebelum chat pertama. Jangan mengirim password, PIN, atau data kartu pembayaran melalui chat.",
    ],
  },
  {
    title: "5. Siapa yang dapat melihat data",
    paragraphs: [
      "Data setiap toko dipisahkan secara teknis dari toko lain. Anggota tim toko hanya dapat melihat data sesuai izin perannya; sebagian data pribadi disamarkan untuk peran terbatas.",
      "Penyedia infrastruktur yang kami pakai (server, penyedia model AI, layanan email bila sudah aktif) hanya menerima data yang diperlukan untuk fungsinya. Admin platform hanya melihat data yang diperlukan untuk dukungan dan verifikasi pembayaran langganan, dan aksesnya tercatat.",
      "Kami dapat mengungkapkan data bila diwajibkan oleh hukum yang berlaku.",
    ],
  },
  {
    title: "6. Penyimpanan & penghapusan",
    paragraphs: [
      "Isi percakapan chat dihapus 30 hari setelah pesan terakhir; ringkasan statistik tanpa isi pesan tetap disimpan untuk laporan toko.",
      "Catatan audit disimpan 1 tahun. Data toko dan data pembeli dihapus paling lambat 30 hari setelah toko ditutup atau setelah permintaan penghapusan yang sah diproses.",
      "Data cadangan (backup) dienkripsi dan diganti secara berkala sesuai jadwal rotasi.",
    ],
  },
  {
    title: "7. Keamanan",
    paragraphs: [
      "Kami memakai koneksi terenkripsi, password ber-hash, enkripsi untuk kunci API dan rahasia verifikasi dua langkah, pemisahan data antar toko di tingkat database, serta pencatatan akses. Tidak ada sistem yang sepenuhnya bebas risiko; bila terjadi insiden yang berdampak pada data Anda, kami akan memberi tahu sesuai ketentuan yang berlaku.",
    ],
  },
  {
    title: "8. Hak Anda",
    paragraphs: [
      "Anda dapat meminta akses, perbaikan, salinan, atau penghapusan data pribadi Anda, serta menarik persetujuan untuk pemrosesan yang berdasarkan persetujuan.",
      "Pembeli: ajukan permintaan kepada toko tempat Anda berbelanja (toko dapat mengekspor atau menghapus data pelanggan). Pemilik toko: ajukan melalui kontak di bawah.",
    ],
  },
  {
    title: "9. Perubahan & kontak",
    paragraphs: [
      "Bila kebijakan ini berubah secara berarti, versi baru akan ditampilkan dan, bila diperlukan, persetujuan ulang akan diminta.",
      `Pertanyaan atau permintaan data: ${LEGAL_CONTACT}.`,
    ],
  },
];

export const TERMS: Section[] = [
  {
    title: "1. Pihak & penerimaan",
    paragraphs: [
      "Syarat ini berlaku untuk pemilik toko yang mendaftar di UMKM Business OS (\"Pemilik Toko\") dan untuk pembeli yang berbelanja di website toko (\"Pembeli\"). Dengan mendaftar atau melakukan pemesanan, Anda menyetujui syarat ini dan Kebijakan Privasi.",
    ],
  },
  {
    title: "2. Untuk Pemilik Toko — akun & tim",
    paragraphs: [
      "Anda bertanggung jawab atas kebenaran data usaha, keamanan password, dan tindakan anggota tim yang Anda undang. Gunakan verifikasi dua langkah untuk peran admin.",
      "Anda menjamin produk, konten website, gambar, dan informasi yang Anda unggah sah, tidak melanggar hak pihak lain, dan tidak melanggar hukum. Kami dapat menangguhkan toko yang terbukti melanggar.",
    ],
  },
  {
    title: "3. Untuk Pemilik Toko — paket & pembayaran",
    paragraphs: [
      "Paket berlangganan: Starter Rp50.000 per 30 hari dan Pro Rp100.000 per 30 hari, dengan batas pemakaian sesuai halaman Langganan. Toko baru mendapat uji coba 14 hari dengan batas paket Starter. Tidak ada paket gratis.",
      "Pembayaran langganan dilakukan secara manual sesuai instruksi di halaman Langganan, lalu diverifikasi admin platform. Masa aktif dimulai atau diperpanjang setelah pembayaran disetujui. Bila pembayaran ditolak, alasan ditampilkan dan Anda dapat mengunggah bukti yang benar.",
      "Bila masa uji coba atau langganan berakhir, website toko, katalog, checkout, dan chat publik dinonaktifkan sampai langganan aktif kembali; data toko tetap tersimpan dan dashboard tetap dapat dibuka. Biaya yang sudah dibayar tidak dikembalikan, kecuali terjadi kesalahan penagihan dari pihak kami.",
      "Harga dan batas paket dapat berubah; perubahan diberitahukan sebelum berlaku dan tidak memengaruhi periode yang sudah dibayar.",
    ],
  },
  {
    title: "4. Untuk Pemilik Toko — asisten AI",
    paragraphs: [
      "Asisten AI membantu menjawab pertanyaan pembeli, tetapi jawabannya dapat keliru. Anda tetap bertanggung jawab atas informasi yang disampaikan kepada pembeli dan dapat memilih mode di mana jawaban AI perlu disetujui staf, atau menonaktifkan AI.",
      "Paket Starter tidak termasuk kuota token AI platform; Anda dapat memakai kunci API penyedia AI milik sendiri, dan biayanya ditagih penyedia tersebut langsung kepada Anda.",
    ],
  },
  {
    title: "5. Untuk Pembeli — pemesanan & pembayaran",
    paragraphs: [
      "Penjual adalah toko yang website-nya Anda kunjungi, bukan UMKM Business OS. Harga, stok, pengiriman, dan pengembalian ditentukan dan menjadi tanggung jawab toko.",
      "Pembayaran dilakukan langsung ke QRIS milik toko. Pesanan diproses setelah toko memverifikasi pembayaran Anda. Pesanan yang belum dibayar sampai batas waktu dibatalkan otomatis dan stoknya dilepas.",
      "Untuk keluhan pesanan, hubungi toko melalui chat atau tiket di website toko. Platform tidak memegang dana pembayaran Anda.",
    ],
  },
  {
    title: "6. Penggunaan yang dilarang",
    paragraphs: [
      "Dilarang: mencoba mengakses data toko lain, mengganggu atau membebani layanan secara sengaja, menyebarkan konten melanggar hukum, penipuan, atau memakai layanan untuk mengirim spam.",
    ],
  },
  {
    title: "7. Ketersediaan & tanggung jawab",
    paragraphs: [
      "Kami berupaya menjaga layanan tetap tersedia dan data dicadangkan secara berkala, namun layanan disediakan sebagaimana adanya dan dapat mengalami gangguan atau pemeliharaan. Sejauh diizinkan hukum, tanggung jawab kami terbatas pada nilai biaya langganan yang Anda bayar dalam 3 bulan terakhir.",
    ],
  },
  {
    title: "8. Penghentian",
    paragraphs: [
      "Pemilik Toko dapat menutup toko kapan saja. Data dihapus sesuai Kebijakan Privasi. Kami dapat menangguhkan atau menutup toko yang melanggar syarat ini setelah pemberitahuan, kecuali untuk pelanggaran berat yang memerlukan tindakan segera.",
    ],
  },
  {
    title: "9. Hukum & perubahan",
    paragraphs: [
      "Syarat ini tunduk pada hukum Republik Indonesia. Perselisihan diupayakan diselesaikan secara musyawarah terlebih dahulu.",
      `Perubahan berarti akan diberitahukan sebelum berlaku. Pertanyaan: ${LEGAL_CONTACT}.`,
    ],
  },
];
