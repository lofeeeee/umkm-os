//! Katalog produk (P3-1, `PRD.md §4.5`). Murni: validasi input, uang, status, varian.
//!
//! Uang disimpan `NUMERIC(14,2)` di PostgreSQL dan dibawa di Rust sebagai [`Money`]
//! (satuan sen, `i64`) — tidak pernah `f64`. API memakai string desimal (`"15000.00"`).

use std::collections::HashSet;

use crate::business::{FieldError, plain_text, push};

/// Harga maksimum Rp 1.000.000.000 (`ck_*_price` di migration sama).
pub const MAX_PRICE_SEN: i64 = 1_000_000_000 * 100;
pub const NAME_MAX: usize = 120;
pub const DESCRIPTION_MAX: usize = 5000;
pub const CATEGORY_NAME_MAX: usize = 60;
pub const MAX_OPTIONS: usize = 3;
pub const MAX_OPTION_VALUES: usize = 20;
pub const OPTION_TEXT_MAX: usize = 40;
pub const MAX_VARIANTS: usize = 100;
pub const MAX_IMAGES: usize = 10;
pub const SKU_MAX: usize = 64;
pub const SLUG_MAX: usize = 80;

/// Nominal rupiah dalam sen. Selalu `0..=MAX_PRICE_SEN`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Money(i64);

impl Money {
    pub const ZERO: Money = Money(0);

    pub fn from_sen(sen: i64) -> Option<Self> {
        (0..=MAX_PRICE_SEN).contains(&sen).then_some(Self(sen))
    }

    /// Desimal ketat: `^\d{1,10}(\.\d{1,2})?$`. Tanpa tanda, eksponen, spasi, atau pemisah ribuan.
    pub fn parse(s: &str) -> Option<Self> {
        let (int, frac) = match s.split_once('.') {
            Some((i, f)) => (i, f),
            None => (s, ""),
        };
        let digits = |x: &str| x.bytes().all(|b| b.is_ascii_digit());
        if int.is_empty() || int.len() > 10 || !digits(int) || frac.len() > 2 || !digits(frac) {
            return None;
        }
        if s.contains('.') && frac.is_empty() {
            return None;
        }
        let whole: i64 = int.parse().ok()?;
        let cents: i64 = match frac.len() {
            0 => 0,
            1 => frac.parse::<i64>().ok()? * 10,
            _ => frac.parse().ok()?,
        };
        Self::from_sen(whole.checked_mul(100)?.checked_add(cents)?)
    }

    pub fn sen(self) -> i64 {
        self.0
    }

    pub fn checked_mul(self, qty: i64) -> Option<i64> {
        self.0.checked_mul(qty)
    }

    /// `"15000.00"`
    pub fn format(self) -> String {
        format_sen(self.0)
    }
}

/// Format nominal sen apa pun (mis. total order yang boleh > [`MAX_PRICE_SEN`]).
pub fn format_sen(sen: i64) -> String {
    let sign = if sen < 0 { "-" } else { "" };
    let a = sen.unsigned_abs();
    format!("{sign}{}.{:02}", a / 100, a % 100)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductStatus {
    Draft,
    Active,
    Archived,
}

impl ProductStatus {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "draft" => Some(Self::Draft),
            "active" => Some(Self::Active),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Active => "active",
            Self::Archived => "archived",
        }
    }

    /// AC-CAT-001: hanya draft→active, active→archived, archived→active.
    pub fn can_transition(self, to: Self) -> bool {
        matches!(
            (self, to),
            (Self::Draft, Self::Active)
                | (Self::Active, Self::Archived)
                | (Self::Archived, Self::Active)
        )
    }
}

/// `[a-z0-9]+(-[a-z0-9]+)*`, ≤ [`SLUG_MAX`].
pub fn is_valid_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= SLUG_MAX
        && !s.starts_with('-')
        && !s.ends_with('-')
        && !s.contains("--")
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Slug dari nama: huruf/angka ASCII, lainnya jadi `-`. `None` bila tidak tersisa karakter.
pub fn slugify(name: &str) -> Option<String> {
    let mut out = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
        if out.len() >= SLUG_MAX {
            break;
        }
    }
    let s = out.trim_end_matches('-').to_owned();
    is_valid_slug(&s).then_some(s)
}

/// SKU dinormalisasi huruf besar; `[A-Z0-9._-]`, 1..=64. Unik per tenant (AC-CAT-003).
pub fn normalize_sku(s: &str) -> Option<String> {
    let s = s.trim().to_ascii_uppercase();
    (!s.is_empty()
        && s.len() <= SKU_MAX
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-')))
    .then_some(s)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionDef {
    pub name: String,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantInput {
    pub id: Option<uuid::Uuid>,
    pub sku: String,
    /// Satu nilai per opsi, urutan sama dengan `options`.
    pub option_values: Vec<String>,
    pub price: String,
    pub promo_price: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    pub id: Option<uuid::Uuid>,
    pub sku: String,
    pub option_values: Vec<String>,
    pub name: String,
    pub price: Money,
    pub promo_price: Option<Money>,
    pub is_active: bool,
}

impl Variant {
    /// Harga yang dibayar: promo bila ada.
    pub fn effective_price(&self) -> Money {
        self.promo_price.unwrap_or(self.price)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductInput {
    pub name: String,
    pub slug: Option<String>,
    pub description: Option<String>,
    pub category_id: Option<uuid::Uuid>,
    pub available: bool,
    pub options: Vec<OptionDef>,
    pub variants: Vec<VariantInput>,
    pub images: Vec<uuid::Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductDraft {
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub category_id: Option<uuid::Uuid>,
    pub available: bool,
    pub options: Vec<OptionDef>,
    pub variants: Vec<Variant>,
    pub images: Vec<uuid::Uuid>,
}

fn price(e: &mut Vec<FieldError>, path: &str, s: &str) -> Option<Money> {
    let m = Money::parse(s);
    if m.is_none() {
        push(e, path, "invalid_price");
    }
    m
}

fn short_text(e: &mut Vec<FieldError>, path: &str, s: &str, max: usize) {
    plain_text(e, path, Some(s), max, true);
    if s.contains('\n') {
        push(e, path, "control_character");
    }
}

/// Validasi lengkap produk (AC-CAT-003/004). Semua error dikumpulkan.
pub fn validate_product(input: ProductInput) -> Result<ProductDraft, Vec<FieldError>> {
    let mut e = vec![];
    let name = input.name.trim().to_owned();
    short_text(&mut e, "name", &name, NAME_MAX);
    let slug = match input
        .slug
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(s) if is_valid_slug(s) => Some(s.to_owned()),
        Some(_) => {
            push(&mut e, "slug", "invalid_slug");
            None
        }
        None => {
            let s = slugify(&name);
            if s.is_none() && !name.is_empty() {
                push(&mut e, "slug", "required");
            }
            s
        }
    };
    let description = input
        .description
        .map(|d| d.trim().to_owned())
        .filter(|d| !d.is_empty());
    plain_text(
        &mut e,
        "description",
        description.as_deref(),
        DESCRIPTION_MAX,
        false,
    );

    // Opsi
    if input.options.len() > MAX_OPTIONS {
        push(&mut e, "options", "too_many");
    }
    let mut names = HashSet::new();
    let options: Vec<OptionDef> = input
        .options
        .into_iter()
        .take(MAX_OPTIONS)
        .enumerate()
        .map(|(i, o)| {
            let p = format!("options[{i}]");
            let name = o.name.trim().to_owned();
            short_text(&mut e, &format!("{p}.name"), &name, OPTION_TEXT_MAX);
            if !names.insert(name.to_lowercase()) {
                push(&mut e, format!("{p}.name"), "duplicate");
            }
            if o.values.is_empty() {
                push(&mut e, format!("{p}.values"), "required");
            }
            if o.values.len() > MAX_OPTION_VALUES {
                push(&mut e, format!("{p}.values"), "too_many");
            }
            let mut seen = HashSet::new();
            let values = o
                .values
                .into_iter()
                .take(MAX_OPTION_VALUES)
                .enumerate()
                .map(|(j, v)| {
                    let v = v.trim().to_owned();
                    short_text(&mut e, &format!("{p}.values[{j}]"), &v, OPTION_TEXT_MAX);
                    if !seen.insert(v.to_lowercase()) {
                        push(&mut e, format!("{p}.values[{j}]"), "duplicate");
                    }
                    v
                })
                .collect();
            OptionDef { name, values }
        })
        .collect();

    // Varian
    if input.variants.is_empty() {
        push(&mut e, "variants", "required");
    }
    if input.variants.len() > MAX_VARIANTS {
        push(&mut e, "variants", "too_many");
    }
    if options.is_empty() && input.variants.len() > 1 {
        push(&mut e, "variants", "options_required_for_multiple_variants");
    }
    let mut skus = HashSet::new();
    let mut combos = HashSet::new();
    let mut ids = HashSet::new();
    let mut variants = vec![];
    for (i, v) in input.variants.into_iter().take(MAX_VARIANTS).enumerate() {
        let p = format!("variants[{i}]");
        if let Some(id) = v.id
            && !ids.insert(id)
        {
            push(&mut e, format!("{p}.id"), "duplicate");
        }
        let sku = normalize_sku(&v.sku);
        match &sku {
            None => push(&mut e, format!("{p}.sku"), "invalid_sku"),
            Some(s) if !skus.insert(s.clone()) => push(&mut e, format!("{p}.sku"), "duplicate_sku"),
            Some(_) => {}
        }
        let values: Vec<String> = v
            .option_values
            .iter()
            .map(|x| x.trim().to_owned())
            .collect();
        if values.len() != options.len() {
            push(&mut e, format!("{p}.option_values"), "count_mismatch");
        } else {
            for (j, (val, opt)) in values.iter().zip(&options).enumerate() {
                if !opt.values.iter().any(|x| x == val) {
                    push(&mut e, format!("{p}.option_values[{j}]"), "unknown_value");
                }
            }
            if !combos.insert(values.iter().map(|x| x.to_lowercase()).collect::<Vec<_>>()) {
                push(
                    &mut e,
                    format!("{p}.option_values"),
                    "duplicate_combination",
                );
            }
        }
        let normal = price(&mut e, &format!("{p}.price"), &v.price);
        let promo = v
            .promo_price
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .and_then(|s| price(&mut e, &format!("{p}.promo_price"), s));
        if let (Some(n), Some(pr)) = (normal, promo)
            && pr >= n
        {
            push(&mut e, format!("{p}.promo_price"), "promo_not_below_price");
        }
        if let (Some(sku), Some(price)) = (sku, normal) {
            let name = if values.is_empty() {
                "Default".to_owned()
            } else {
                values.join(" / ")
            };
            variants.push(Variant {
                id: v.id,
                sku,
                name,
                option_values: values,
                price,
                promo_price: promo,
                is_active: v.is_active,
            });
        }
    }
    if !variants.is_empty() && !variants.iter().any(|v| v.is_active) && e.is_empty() {
        push(&mut e, "variants", "no_active_variant");
    }

    if input.images.len() > MAX_IMAGES {
        push(&mut e, "images", "too_many");
    }
    let mut seen = HashSet::new();
    if input.images.iter().any(|i| !seen.insert(*i)) {
        push(&mut e, "images", "duplicate");
    }

    match slug {
        Some(slug) if e.is_empty() => Ok(ProductDraft {
            name,
            slug,
            description,
            category_id: input.category_id,
            available: input.available,
            options,
            variants,
            images: input.images,
        }),
        _ => Err(e),
    }
}

/// Nama kategori: teks satu baris 1..=60.
pub fn validate_category_name(name: &str) -> Result<String, Vec<FieldError>> {
    let mut e = vec![];
    let n = name.trim().to_owned();
    short_text(&mut e, "name", &n, CATEGORY_NAME_MAX);
    if e.is_empty() { Ok(n) } else { Err(e) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variant(sku: &str, vals: &[&str], price: &str, promo: Option<&str>) -> VariantInput {
        VariantInput {
            id: None,
            sku: sku.into(),
            option_values: vals.iter().map(|s| (*s).to_owned()).collect(),
            price: price.into(),
            promo_price: promo.map(Into::into),
            is_active: true,
        }
    }

    fn input(options: Vec<OptionDef>, variants: Vec<VariantInput>) -> ProductInput {
        ProductInput {
            name: "Kopi Susu Gula Aren".into(),
            slug: None,
            description: None,
            category_id: None,
            available: true,
            options,
            variants,
            images: vec![],
        }
    }

    fn codes(r: Result<ProductDraft, Vec<FieldError>>) -> Vec<(String, &'static str)> {
        r.err()
            .unwrap()
            .into_iter()
            .map(|e| (e.path, e.code))
            .collect()
    }

    #[test]
    fn money_parse_strict() {
        assert_eq!(Money::parse("15000").unwrap().sen(), 1_500_000);
        assert_eq!(Money::parse("15000.5").unwrap().sen(), 1_500_050);
        assert_eq!(Money::parse("0.01").unwrap().sen(), 1);
        assert_eq!(Money::parse("1000000000").unwrap().sen(), MAX_PRICE_SEN);
        for bad in [
            "",
            "-1",
            "1e3",
            "1.",
            ".5",
            "1.234",
            "1,000",
            " 1",
            "1000000000.01",
            "+5",
            "NaN",
            "１",
        ] {
            assert!(Money::parse(bad).is_none(), "{bad}");
        }
        assert_eq!(Money::parse("12.3").unwrap().format(), "12.30");
        assert_eq!(format_sen(-150), "-1.50");
    }

    #[test]
    fn status_transitions() {
        use ProductStatus::*;
        let all = [Draft, Active, Archived];
        let allowed: Vec<_> = all
            .iter()
            .flat_map(|a| all.iter().map(move |b| (*a, *b)))
            .filter(|(a, b)| a.can_transition(*b))
            .collect();
        assert_eq!(
            allowed,
            vec![(Draft, Active), (Active, Archived), (Archived, Active)]
        );
    }

    #[test]
    fn slug_and_sku() {
        assert_eq!(
            slugify("Kopi Susu — Gula Aren!").as_deref(),
            Some("kopi-susu-gula-aren")
        );
        assert_eq!(slugify("☕☕"), None);
        assert!(
            is_valid_slug("a-1")
                && !is_valid_slug("a--1")
                && !is_valid_slug("-a")
                && !is_valid_slug("A")
        );
        assert_eq!(normalize_sku(" ks-01.a ").as_deref(), Some("KS-01.A"));
        assert!(normalize_sku("a b").is_none() && normalize_sku("").is_none());
    }

    #[test]
    fn simple_product_ok() {
        let d = validate_product(input(
            vec![],
            vec![variant("ks-1", &[], "18000", Some("15000"))],
        ))
        .unwrap();
        assert_eq!(d.slug, "kopi-susu-gula-aren");
        assert_eq!(d.variants[0].name, "Default");
        assert_eq!(d.variants[0].effective_price().sen(), 1_500_000);
    }

    #[test]
    fn promo_and_negative_rejected() {
        // AC-CAT-004
        let c = codes(validate_product(input(
            vec![],
            vec![variant("a", &[], "100", Some("150"))],
        )));
        assert!(c.contains(&("variants[0].promo_price".into(), "promo_not_below_price")));
        let c = codes(validate_product(input(
            vec![],
            vec![variant("a", &[], "100", Some("100"))],
        )));
        assert!(c.contains(&("variants[0].promo_price".into(), "promo_not_below_price")));
        let c = codes(validate_product(input(
            vec![],
            vec![variant("a", &[], "-1", None)],
        )));
        assert!(c.contains(&("variants[0].price".into(), "invalid_price")));
    }

    #[test]
    fn variants_follow_options() {
        let opts = vec![
            OptionDef {
                name: "Ukuran".into(),
                values: vec!["S".into(), "L".into()],
            },
            OptionDef {
                name: "Suhu".into(),
                values: vec!["Panas".into(), "Dingin".into()],
            },
        ];
        let ok = validate_product(input(
            opts.clone(),
            vec![
                variant("a", &["S", "Panas"], "10", None),
                variant("b", &["L", "Dingin"], "12", None),
            ],
        ))
        .unwrap();
        assert_eq!(ok.variants[1].name, "L / Dingin");

        let c = codes(validate_product(input(
            opts,
            vec![
                variant("a", &["S", "Panas"], "10", None),
                variant("A", &["s", "panas"], "10", None),
                variant("c", &["XL", "Panas"], "10", None),
                variant("d", &["S"], "10", None),
            ],
        )));
        assert!(c.contains(&("variants[1].sku".into(), "duplicate_sku")));
        assert!(c.contains(&("variants[1].option_values[0]".into(), "unknown_value")));
        assert!(c.contains(&("variants[2].option_values[0]".into(), "unknown_value")));
        assert!(c.contains(&("variants[3].option_values".into(), "count_mismatch")));
    }

    #[test]
    fn structural_limits() {
        let c = codes(validate_product(input(vec![], vec![])));
        assert!(c.contains(&("variants".into(), "required")));
        let c = codes(validate_product(input(
            vec![],
            vec![variant("a", &[], "1", None), variant("b", &[], "1", None)],
        )));
        assert!(c.contains(&("variants".into(), "options_required_for_multiple_variants")));
        let mut v = variant("a", &[], "1", None);
        v.is_active = false;
        let c = codes(validate_product(input(vec![], vec![v])));
        assert!(c.contains(&("variants".into(), "no_active_variant")));
        let mut i = input(vec![], vec![variant("a", &[], "1", None)]);
        let img = uuid::Uuid::from_u128(7);
        i.images = vec![img, img];
        i.name = "<b>\u{7}".into();
        let c = codes(validate_product(i));
        assert!(c.contains(&("images".into(), "duplicate")));
        assert!(c.contains(&("name".into(), "control_character")));
    }
}
