// `sqlx::migrate!` meng-embed migrations saat kompilasi tetapi tidak melacak FILE BARU
// di direktori. Tanpa ini artefak build lama (mis. cache CI atau unifikasi fitur di
// workspace) tetap membawa MIGRATOR basi. Lihat docs/evidence/P1-1.txt.
fn main() {
    println!("cargo:rerun-if-changed=../../migrations");
}
