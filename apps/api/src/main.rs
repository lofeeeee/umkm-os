//! umkm-api entrypoint: `umkm-api [serve|migrate|healthcheck]`.

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match umkm_api::bootstrap::main_with_args(&args).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            // Tracing mungkin belum terinisialisasi bila config gagal dimuat.
            eprintln!("umkm-api: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
