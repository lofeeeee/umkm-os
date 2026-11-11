//! umkm-worker — entrypoint. Kerangka P0-1; runtime ditambahkan di P0-3 (api) / P1-6 (worker).

fn main() {
    let layers = [
        umkm_shared::CRATE,
        umkm_application::CRATE,
        umkm_infrastructure::CRATE,
        umkm_database::CRATE,
    ];
    println!("umkm-worker skeleton: {}", layers.join(", "));
}
