//! umkm-api entrypoint.

#[tokio::main]
async fn main() -> std::process::ExitCode {
    match umkm_api::bootstrap::run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            // Tracing mungkin belum terinisialisasi bila config gagal dimuat.
            eprintln!("umkm-api: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
