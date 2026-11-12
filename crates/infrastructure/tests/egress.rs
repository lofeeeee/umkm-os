//! P5-3: klien keluar terkendali terhadap server HTTP lokal nyata (kontrol 3–9 `PRD.md §6.5`).
//! Server lokal = alamat loopback, jadi jalur sukses memakai `EgressPolicy::operator` (kontrol 9)
//! sementara kebijakan tenant harus menolaknya tanpa satu pun request sampai.

use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{
    Router,
    body::Body,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{any, get},
};
use umkm_infrastructure::http_client::{Egress, EgressPolicy, Resolver, SsrfError};

#[derive(Clone, Default)]
struct Seen {
    hits: Arc<AtomicUsize>,
    secret_hits: Arc<AtomicUsize>,
    auth: Arc<Mutex<Vec<String>>>,
}

async fn server() -> (SocketAddr, Seen) {
    let seen = Seen::default();
    let s = seen.clone();
    let s2 = seen.clone();
    let app = Router::new()
        .route(
            "/v1/chat",
            any(move |h: HeaderMap| {
                let s = s.clone();
                async move {
                    s.hits.fetch_add(1, Ordering::SeqCst);
                    if let Some(a) = h.get(header::AUTHORIZATION) {
                        s.auth.lock().unwrap().push(a.to_str().unwrap().to_owned());
                    }
                    "{\"ok\":true}"
                }
            }),
        )
        .route(
            "/redir",
            get(|| async { (StatusCode::FOUND, [(header::LOCATION, "/secret")]) }),
        )
        .route(
            "/secret",
            get(move || {
                let s = s2.clone();
                async move {
                    s.secret_hits.fetch_add(1, Ordering::SeqCst);
                    "metadata"
                }
            }),
        )
        .route("/big", get(|| async { vec![b'x'; 4096] }))
        .route(
            "/stream",
            get(|| async {
                let chunks = (0..8).map(|_| Ok::<_, std::io::Error>(vec![b'y'; 512]));
                Response::new(Body::from_stream(futures_util::stream::iter(chunks))).into_response()
            }),
        )
        .route(
            "/slow",
            get(|| async {
                tokio::time::sleep(Duration::from_secs(3)).await;
                "late"
            }),
        );
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    (addr, seen)
}

fn fake_dns(host: &str, ips: &[&str]) -> Resolver {
    Resolver::Static(HashMap::from([(
        host.to_owned(),
        ips.iter().map(|i| i.parse::<IpAddr>().unwrap()).collect(),
    )]))
}

fn operator(max_bytes: usize, timeout_ms: u64) -> EgressPolicy {
    EgressPolicy {
        max_response_bytes: max_bytes,
        ..EgressPolicy::operator(Duration::from_millis(timeout_ms))
    }
}

#[tokio::test]
async fn tenant_policy_never_reaches_local_or_mixed_targets_and_never_sends_key() {
    let (addr, seen) = server().await;
    let tenant = Egress::new(
        EgressPolicy::tenant(Duration::from_secs(2)),
        Resolver::System,
        4,
    );
    let port = addr.port();
    for (url, want) in [
        (
            format!("http://127.0.0.1:{port}/v1/chat"),
            SsrfError::SchemeNotAllowed,
        ),
        (
            format!("https://127.0.0.1:{port}/v1/chat"),
            SsrfError::ForbiddenAddress,
        ),
        (
            format!("https://localhost:{port}/v1/chat"),
            SsrfError::ForbiddenHost,
        ),
        (
            format!("https://[::1]:{port}/v1/chat"),
            SsrfError::ForbiddenAddress,
        ),
    ] {
        assert_eq!(
            tenant
                .post_json(&url, Some("sk-RAHASIA"), b"{}".to_vec())
                .await
                .unwrap_err(),
            want,
            "{url}"
        );
    }
    // Kontrol 4: nama publik yang SALAH SATU IP-nya privat → tolak (bukan pilih IP yang aman).
    let mixed = Egress::new(
        EgressPolicy::tenant(Duration::from_secs(2)),
        fake_dns("api.mixed.test", &["8.8.8.8", "127.0.0.1"]),
        4,
    );
    assert_eq!(
        mixed
            .post_json(
                &format!("https://api.mixed.test:{port}/v1/chat"),
                Some("sk-RAHASIA"),
                b"{}".to_vec()
            )
            .await
            .unwrap_err(),
        SsrfError::ForbiddenAddress
    );
    let rebind = Egress::new(
        EgressPolicy::tenant(Duration::from_secs(2)),
        fake_dns("api.rebind.test", &["169.254.169.254"]),
        4,
    );
    assert_eq!(
        rebind
            .get("https://api.rebind.test/v1/models", Some("sk-RAHASIA"))
            .await
            .unwrap_err(),
        SsrfError::ForbiddenAddress
    );
    // Kontrol 8: tidak satu pun request (apalagi kunci) sampai ke server.
    assert_eq!(seen.hits.load(Ordering::SeqCst), 0);
    assert!(seen.auth.lock().unwrap().is_empty());
}

#[tokio::test]
async fn connection_is_pinned_to_validated_address() {
    let (addr, seen) = server().await;
    // Nama ini tidak ada di DNS sistem: sukses hanya mungkin bila koneksi memakai IP tervalidasi (kontrol 5).
    let e = Egress::new(
        operator(1 << 20, 2000),
        fake_dns("llm.pinned.test", &["127.0.0.1"]),
        4,
    );
    let r = e
        .post_json(
            &format!("http://llm.pinned.test:{}/v1/chat", addr.port()),
            Some("sk-OK"),
            b"{}".to_vec(),
        )
        .await
        .unwrap();
    assert_eq!(
        (r.status, r.body.as_slice()),
        (200, b"{\"ok\":true}".as_slice())
    );
    assert_eq!(seen.auth.lock().unwrap().as_slice(), ["Bearer sk-OK"]);
    // Operator pun tidak boleh ke metadata.
    let meta = Egress::new(
        operator(1 << 20, 2000),
        fake_dns("meta.test", &["169.254.169.254"]),
        4,
    );
    assert_eq!(
        meta.get("http://meta.test/latest", None).await.unwrap_err(),
        SsrfError::ForbiddenAddress
    );
}

#[tokio::test]
async fn redirects_size_timeout_and_concurrency_are_bounded() {
    let (addr, seen) = server().await;
    let base = format!("http://127.0.0.1:{}", addr.port());
    let e = Egress::new(operator(1024, 500), Resolver::System, 1);
    // Kontrol 6: 302 → error, target redirect tidak pernah diminta.
    assert_eq!(
        e.get(&format!("{base}/redir"), None).await.unwrap_err(),
        SsrfError::Redirect
    );
    assert_eq!(seen.secret_hits.load(Ordering::SeqCst), 0);
    // Kontrol 7: ukuran (dengan Content-Length & chunked tanpa Content-Length), timeout.
    assert_eq!(
        e.get(&format!("{base}/big"), None).await.unwrap_err(),
        SsrfError::TooLarge
    );
    assert_eq!(
        e.get(&format!("{base}/stream"), None).await.unwrap_err(),
        SsrfError::TooLarge
    );
    let t0 = std::time::Instant::now();
    assert_eq!(
        e.get(&format!("{base}/slow"), None).await.unwrap_err(),
        SsrfError::Timeout
    );
    assert!(t0.elapsed() < Duration::from_secs(2));
    // Kontrol 7: konkurensi 1 → permintaan kedua saat yang pertama berjalan ditolak Busy.
    let e2 = Egress::new(operator(1024, 2000), Resolver::System, 1);
    let slow = {
        let e2 = e2.clone();
        let u = format!("{base}/slow");
        tokio::spawn(async move { e2.get(&u, None).await })
    };
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        e2.get(&format!("{base}/v1/chat"), None).await.unwrap_err(),
        SsrfError::Busy
    );
    assert_eq!(slow.await.unwrap().unwrap_err(), SsrfError::Timeout);
}
