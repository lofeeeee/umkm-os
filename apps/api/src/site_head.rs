//! `<head>` situs toko per Host (ADR-0018, P9-5): judul, deskripsi, kanonik, Open Graph, favicon, dan
//! JSON-LD disisipkan ke shell statis hasil build frontend, supaya crawler pratinjau tautan (WhatsApp,
//! Facebook) yang TIDAK menjalankan JavaScript tetap melihat judul & gambar toko. Juga `robots.txt` dan
//! `sitemap.xml` per toko. Sumber data: versi TERBIT + profil bisnis (draf tidak pernah bocor).
//!
//! Keamanan: semua nilai di-escape untuk konteks HTML; JSON-LD adalah blok data (`application/ld+json`,
//! tidak dieksekusi) yang di-escape untuk konteks `<script>` (`<`, `>`, `&` → `\u003c` dst.), sehingga
//! teks pemilik toko tidak bisa menutup elemen atau menyuntik markup.

use std::collections::BTreeSet;

use axum::{
    Router,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use serde_json::{Value, json};
use umkm_infrastructure::{business_repo, catalog_repo, website_repo};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

use crate::{
    AppState,
    business::{BusinessView, to_view},
    catalog::public_view,
    http_util::{internal, pool},
    middleware::tenant::HostTenant,
    website::signed_assets,
};

/// Konfigurasi proses: shell statis + origin publik API (URL absolut gambar) + skema situs.
#[derive(Debug, Clone, Default)]
pub struct SiteHead {
    /// Isi `site.html` hasil build frontend. `None` = fitur nonaktif (Caddy menyajikan file statis).
    pub shell: Option<String>,
    pub api_origin: String,
    pub scheme: String,
}

/// Data `<head>` satu halaman (sudah final; belum di-escape).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HeadMeta {
    pub title: String,
    pub description: String,
    pub url: String,
    pub site_name: String,
    pub og_type: &'static str,
    pub image: Option<String>,
    pub favicon: Option<String>,
    pub json_ld: Option<Value>,
}

/// Escape untuk teks & nilai atribut HTML (dikutip ganda).
pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            c if c.is_control() && c != '\n' => {}
            c => o.push(c),
        }
    }
    o
}

/// JSON aman di dalam `<script type="application/ld+json">`.
pub fn json_for_script(v: &Value) -> String {
    serde_json::to_string(v)
        .unwrap_or_else(|_| "{}".into())
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

/// Potong ke `max` karakter (deskripsi meta), tanpa memotong di tengah karakter.
fn clip(s: &str, max: usize) -> String {
    let t = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() <= max {
        t
    } else {
        let mut c: String = t.chars().take(max.saturating_sub(1)).collect();
        c.push('…');
        c
    }
}

pub fn render_head(m: &HeadMeta) -> String {
    let mut h = String::new();
    h.push_str(&format!("<title>{}</title>", esc(&m.title)));
    if !m.description.is_empty() {
        h.push_str(&format!(
            "<meta name=\"description\" content=\"{}\">",
            esc(&m.description)
        ));
    }
    h.push_str(&format!(
        "<link rel=\"canonical\" href=\"{}\">",
        esc(&m.url)
    ));
    for (p, v) in [
        ("og:title", m.title.as_str()),
        ("og:description", m.description.as_str()),
        ("og:url", m.url.as_str()),
        ("og:site_name", m.site_name.as_str()),
        ("og:type", m.og_type),
        ("og:locale", "id_ID"),
    ] {
        if !v.is_empty() {
            h.push_str(&format!("<meta property=\"{p}\" content=\"{}\">", esc(v)));
        }
    }
    if let Some(img) = &m.image {
        h.push_str(&format!(
            "<meta property=\"og:image\" content=\"{}\">",
            esc(img)
        ));
        h.push_str("<meta name=\"twitter:card\" content=\"summary_large_image\">");
    } else {
        h.push_str("<meta name=\"twitter:card\" content=\"summary\">");
    }
    if let Some(f) = &m.favicon {
        h.push_str(&format!("<link rel=\"icon\" href=\"{}\">", esc(f)));
    }
    if let Some(ld) = &m.json_ld {
        h.push_str(&format!(
            "<script type=\"application/ld+json\">{}</script>",
            json_for_script(ld)
        ));
    }
    h
}

/// Sisipkan `head` ke shell: buang `<title>` & meta description bawaan shell, lalu taruh sebelum `</head>`.
pub fn inject(shell: &str, head: &str) -> String {
    let mut s = shell.to_owned();
    if let (Some(a), Some(b)) = (s.find("<title>"), s.find("</title>"))
        && a < b
    {
        s.replace_range(a..b + "</title>".len(), "");
    }
    while let Some(a) = s.find("<meta name=\"description\"") {
        let Some(len) = s[a..].find('>') else { break };
        s.replace_range(a..a + len + 1, "");
    }
    match s.find("</head>") {
        Some(i) => {
            s.insert_str(i, head);
            s
        }
        None => format!("{head}{s}"),
    }
}

const DAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// JSON-LD LocalBusiness dari profil bisnis (schema.org; Google Search Central rich result).
pub fn local_business(b: &BusinessView, url: &str, image: Option<&str>) -> Value {
    let p = &b.profile;
    let mut v = json!({"@context": "https://schema.org", "@type": "LocalBusiness", "name": p.name, "url": url});
    if let Some(d) = p.description.as_deref().filter(|d| !d.trim().is_empty()) {
        v["description"] = json!(clip(d, 300));
    }
    if let Some(t) = p
        .phone
        .as_deref()
        .or(p.whatsapp.as_deref())
        .filter(|t| !t.trim().is_empty())
    {
        v["telephone"] = json!(t);
    }
    if let Some(i) = image {
        v["image"] = json!(i);
    }
    if let Some(a) = b.addresses.first() {
        v["address"] =
            json!({"@type": "PostalAddress", "streetAddress": a.address, "addressCountry": "ID"});
        if let (Some(lat), Some(lng)) = (a.latitude, a.longitude) {
            v["geo"] = json!({"@type": "GeoCoordinates", "latitude": lat, "longitude": lng});
        }
    }
    let hours: Vec<Value> = b
        .hours
        .iter()
        .filter_map(|h| {
            let d = DAYS.get(usize::from(h.day_of_week).checked_sub(1)?)?;
            Some(json!({"@type": "OpeningHoursSpecification", "dayOfWeek": format!("https://schema.org/{d}"), "opens": h.open, "closes": h.close}))
        })
        .collect();
    if !hours.is_empty() {
        v["openingHoursSpecification"] = json!(hours);
    }
    v
}

// ------------------------------------------------------------------ handler

#[derive(Debug, Deserialize)]
pub struct DocQuery {
    path: Option<String>,
}

fn host_of(h: &HeaderMap) -> String {
    h.get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(|s| {
            s.chars()
                .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
                .collect()
        })
        .unwrap_or_default()
}

/// Path dokumen aman untuk URL kanonik: hanya `[A-Za-z0-9/_-]`, maks 200.
fn clean_path(p: Option<&str>) -> String {
    let p = p.unwrap_or("/");
    if !p.starts_with('/')
        || p.len() > 200
        || !p
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_'))
    {
        return "/".into();
    }
    p.into()
}

fn cached(body: String, ctype: &'static str) -> Response {
    let mut r = (StatusCode::OK, body).into_response();
    let h = r.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(ctype));
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=60"),
    );
    h.insert(header::VARY, HeaderValue::from_static("Host"));
    r
}

/// `GET /api/v1/public/document?path=/…` → shell HTML dengan `<head>` toko. Gagal/nonaktif → 404,
/// dan Caddy jatuh balik ke shell statis (situs tetap tampil, hanya tanpa meta).
pub async fn document(
    State(state): State<AppState>,
    HostTenant(t): HostTenant,
    headers: HeaderMap,
    Query(q): Query<DocQuery>,
) -> Response {
    let Some(shell) = state.site.shell.clone() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match build_meta(
        &state,
        t,
        &host_of(&headers),
        &clean_path(q.path.as_deref()),
    )
    .await
    {
        Ok(Some(meta)) => cached(
            inject(&shell, &render_head(&meta)),
            "text/html; charset=utf-8",
        ),
        // Langganan habis / toko tanpa versi terbit: shell apa adanya.
        Ok(None) => cached(shell, "text/html; charset=utf-8"),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn build_meta(
    state: &AppState,
    t: TenantId,
    host: &str,
    path: &str,
) -> Result<Option<HeadMeta>, crate::ApiError> {
    if crate::billing::require_operational(state, t).await.is_err() {
        return Ok(None);
    }
    let p = pool(state)?;
    let Some(info) = business_repo::load(p, t).await.map_err(internal)? else {
        return Ok(None);
    };
    let biz = to_view(info);
    let content = website_repo::published_content(p, t, website_repo::HOME)
        .await
        .map_err(internal)?
        .map(|(_, c)| c);
    let seo = content
        .as_ref()
        .and_then(|c| c.get("seo"))
        .cloned()
        .unwrap_or(Value::Null);
    let seo_str = |k: &str| {
        seo.get(k)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };

    // Aset yang mungkin dipakai: gambar SEO, favicon, sampul/logo profil.
    let refs: Vec<String> = [
        seo_str("image"),
        seo_str("favicon"),
        biz.profile.cover.clone(),
        biz.profile.logo.clone(),
    ]
    .into_iter()
    .flatten()
    .collect();
    let ids: BTreeSet<Uuid> = refs
        .iter()
        .filter_map(|r| r.strip_prefix("asset:"))
        .filter_map(|x| Uuid::parse_str(x).ok())
        .collect();
    let signed = signed_assets(state, t, ids).await?;
    let abs = |r: Option<String>| {
        r.and_then(|r| {
            signed
                .get(&r)
                .map(|p| format!("{}{p}", state.site.api_origin))
        })
    };

    let scheme = if state.site.scheme.is_empty() {
        "https"
    } else {
        state.site.scheme.as_str()
    };
    let site_url = format!("{scheme}://{host}");
    let url = format!("{site_url}{path}");
    let name = biz.profile.name.clone();
    let store_image = abs(seo_str("image"))
        .or_else(|| abs(biz.profile.cover.clone()))
        .or_else(|| abs(biz.profile.logo.clone()));
    let favicon = abs(seo_str("favicon")).or_else(|| abs(biz.profile.logo.clone()));
    let store_desc = seo_str("description")
        .or_else(|| biz.profile.description.clone())
        .unwrap_or_default();

    // Halaman produk: judul/gambar/harga produk (pratinjau tautan produk di WhatsApp).
    if let Some(slug) = path
        .strip_prefix("/produk/")
        .filter(|s| umkm_domain::catalog::is_valid_slug(s))
        && let Some(prod) = catalog_repo::public_product(p, t, slug)
            .await
            .map_err(internal)?
    {
        let v = public_view(state, t, vec![prod]).await?.remove(0);
        let image = v
            .images
            .first()
            .map(|i| format!("{}{i}", state.site.api_origin))
            .or(store_image);
        let price = v
            .variants
            .first()
            .map(|x| x.promo_price.clone().unwrap_or_else(|| x.price.clone()));
        let mut ld = json!({"@context": "https://schema.org", "@type": "Product", "name": v.name, "url": url});
        if let Some(d) = v.description.as_deref() {
            ld["description"] = json!(clip(d, 300));
        }
        if let Some(i) = &image {
            ld["image"] = json!(i);
        }
        if let Some(pr) = &price {
            ld["offers"] = json!({"@type": "Offer", "price": pr, "priceCurrency": "IDR",
                "availability": if v.available { "https://schema.org/InStock" } else { "https://schema.org/OutOfStock" }});
        }
        return Ok(Some(HeadMeta {
            title: format!("{} — {name}", v.name),
            description: clip(v.description.as_deref().unwrap_or(&store_desc), 160),
            url,
            site_name: name,
            og_type: "product",
            image,
            favicon,
            json_ld: Some(ld),
        }));
    }

    let title = seo_str("title").unwrap_or_else(|| name.clone());
    Ok(Some(HeadMeta {
        json_ld: Some(local_business(&biz, &site_url, store_image.as_deref())),
        title,
        description: clip(&store_desc, 160),
        url,
        site_name: name,
        og_type: "website",
        image: store_image,
        favicon,
    }))
}

/// `GET /api/v1/public/robots.txt` — izinkan semua, tunjuk sitemap toko.
pub async fn robots(
    HostTenant(_t): HostTenant,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let scheme = if state.site.scheme.is_empty() {
        "https"
    } else {
        state.site.scheme.as_str()
    };
    let body = format!(
        "User-agent: *\nAllow: /\nDisallow: /keranjang\nDisallow: /akun\nDisallow: /pesanan\nSitemap: {scheme}://{}/sitemap.xml\n",
        host_of(&headers)
    );
    cached(body, "text/plain; charset=utf-8")
}

/// `GET /api/v1/public/sitemap.xml` — beranda + produk aktif (maks 500).
pub async fn sitemap(
    State(state): State<AppState>,
    HostTenant(t): HostTenant,
    headers: HeaderMap,
) -> Response {
    let scheme = if state.site.scheme.is_empty() {
        "https"
    } else {
        state.site.scheme.as_str()
    };
    let base = format!("{scheme}://{}", host_of(&headers));
    let mut urls = vec![format!("{base}/")];
    if crate::billing::require_operational(&state, t).await.is_ok()
        && let Ok(p) = pool(&state)
        && let Ok((list, _)) = catalog_repo::public_products(p, t, None, 500, 0).await
    {
        urls.extend(
            list.into_iter()
                .map(|x| format!("{base}/produk/{}", x.slug)),
        );
    }
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    for u in urls {
        xml.push_str(&format!("  <url><loc>{}</loc></url>\n", esc(&u)));
    }
    xml.push_str("</urlset>\n");
    cached(xml, "application/xml; charset=utf-8")
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/public/document", get(document))
        .route("/api/v1/public/robots.txt", get(robots))
        .route("/api/v1/public/sitemap.xml", get(sitemap))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> HeadMeta {
        HeadMeta {
            title: "Kopi \"Senja\" </title><script>alert(1)</script>".into(),
            description: "Kedai <b>kopi</b> & roti".into(),
            url: "https://kopisenja.example/".into(),
            site_name: "Kopi Senja".into(),
            og_type: "website",
            image: Some("https://api.example/f/1?sig=a&b=c".into()),
            favicon: None,
            json_ld: Some(json!({"name": "</script><script>alert(1)</script>"})),
        }
    }

    #[test]
    fn head_escapes_text_attributes_and_json_ld() {
        let h = render_head(&meta());
        assert!(h.contains("<title>Kopi &quot;Senja&quot; &lt;/title&gt;&lt;script&gt;alert(1)&lt;/script&gt;</title>"));
        assert!(h.contains("content=\"Kedai &lt;b&gt;kopi&lt;/b&gt; &amp; roti\""));
        assert!(h.contains("og:image\" content=\"https://api.example/f/1?sig=a&amp;b=c\""));
        // JSON-LD tidak bisa menutup <script>: hanya satu </script> (penutup blok sendiri).
        assert_eq!(h.matches("</script>").count(), 1);
        assert!(h.contains("\\u003c/script\\u003e"));
        assert!(!h.contains("<script>alert"));
        assert!(h.contains("twitter:card\" content=\"summary_large_image\""));
    }

    #[test]
    fn inject_replaces_shell_title_and_description() {
        let shell = "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>UMKM OS</title><meta name=\"description\" content=\"x\"/><script src=\"/_next/a.js\"></script></head><body></body></html>";
        let out = inject(shell, &render_head(&meta()));
        assert_eq!(out.matches("<title>").count(), 1);
        assert!(!out.contains("<title>UMKM OS</title>"));
        assert_eq!(out.matches("name=\"description\"").count(), 1);
        assert!(out.find("og:title").unwrap() < out.find("</head>").unwrap());
        assert!(
            out.contains("<script src=\"/_next/a.js\"></script>"),
            "skrip shell tetap utuh"
        );
    }

    #[test]
    fn clean_path_and_host_reject_injection() {
        assert_eq!(clean_path(Some("/produk/kopi-susu")), "/produk/kopi-susu");
        assert_eq!(clean_path(Some("/\"><script>")), "/");
        assert_eq!(clean_path(Some("https://evil")), "/");
        assert_eq!(clean_path(None), "/");
        let mut h = HeaderMap::new();
        h.insert(
            header::HOST,
            HeaderValue::from_static("kopi.ryasai.my.id\"><x"),
        );
        assert_eq!(host_of(&h), "kopi.ryasai.my.idx");
    }

    #[test]
    fn clip_and_esc_edges() {
        assert_eq!(clip("a  b\nc", 10), "a b c");
        assert_eq!(clip(&"x".repeat(200), 160).chars().count(), 160);
        assert_eq!(esc("a\u{0}b'"), "ab&#39;");
    }
}
