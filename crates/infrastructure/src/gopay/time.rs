//! Konversi waktu ISO8601 ↔ epoch-ms tanpa dependensi crate waktu (project memakai `now()` Postgres
//! untuk waktu domain; di sini hanya untuk membangun query & mengurai stempel transaksi GoBiz).
//! Algoritma days_from_civil / civil_from_days: Howard Hinnant (public domain).

/// Hari sejak 1970-01-01 untuk tanggal (y, m, d) kalender Gregorian proleptik.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = ((153 * (if m > 2 { m - 3 } else { m + 9 }) as i64 + 2) / 5) + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Kebalikannya: (tahun, bulan, hari) dari hari sejak 1970-01-01.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Format epoch-ms (UTC) jadi `YYYY-MM-DDTHH:MM:SSZ`.
pub fn iso8601_utc(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86400);
    let tod = secs.rem_euclid(86400);
    let (y, mo, d) = civil_from_days(days);
    let (h, mi, s) = (tod / 3600, (tod % 3600) / 60, tod % 60);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// Urai stempel ISO8601 jadi epoch-ms. Mendukung `Z`, offset `±HH:MM`, dan pecahan detik.
/// Tahun 4 digit, pemisah `T` atau spasi. Mengembalikan `None` bila tak dikenali.
pub fn parse_iso8601_ms(s: &str) -> Option<i64> {
    let s = s.trim();
    let b = s.as_bytes();
    if s.len() < 19 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let num = |a: usize, z: usize| s.get(a..z)?.parse::<i64>().ok();
    let year = num(0, 4)?;
    let mon = num(5, 7)? as u32;
    let day = num(8, 10)? as u32;
    if !(b[10] == b'T' || b[10] == b' ') {
        return None;
    }
    let hour = num(11, 13)?;
    let min = num(14, 16)?;
    let sec = num(17, 19)?;
    if !(1..=12).contains(&mon) || !(1..=31).contains(&day) {
        return None;
    }

    // Sisa: opsional pecahan detik lalu zona waktu.
    let mut rest = &s[19..];
    let mut millis = 0i64;
    if let Some(r) = rest.strip_prefix('.') {
        let frac_len = r.bytes().take_while(u8::is_ascii_digit).count();
        let frac = &r[..frac_len];
        // Ambil 3 digit pertama sebagai milidetik.
        let mut ms3 = String::from(frac);
        ms3.truncate(3);
        while ms3.len() < 3 {
            ms3.push('0');
        }
        millis = ms3.parse().ok()?;
        rest = &r[frac_len..];
    }

    // Zona waktu: Z (UTC) atau ±HH:MM / ±HHMM. Kosong → anggap UTC.
    let offset_min: i64 = if rest.is_empty() || rest == "Z" || rest == "z" {
        0
    } else {
        let sign = match rest.as_bytes()[0] {
            b'+' => 1,
            b'-' => -1,
            _ => return None,
        };
        let digits: String = rest[1..].chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.len() < 4 {
            return None;
        }
        let oh: i64 = digits[0..2].parse().ok()?;
        let om: i64 = digits[2..4].parse().ok()?;
        sign * (oh * 60 + om)
    };

    let days = days_from_civil(year, mon, day);
    let secs = days * 86400 + hour * 3600 + min * 60 + sec - offset_min * 60;
    Some(secs * 1000 + millis)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_epoch() {
        assert_eq!(iso8601_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601_utc(1_700_000_000_000), "2023-11-14T22:13:20Z");
    }

    #[test]
    fn roundtrip_utc() {
        assert_eq!(parse_iso8601_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            parse_iso8601_ms("2023-11-14T22:13:20Z"),
            Some(1_700_000_000_000)
        );
    }

    #[test]
    fn parses_offset_and_fraction() {
        // +07:00 WIB sama dengan 15:13:20Z.
        assert_eq!(
            parse_iso8601_ms("2023-11-15T05:13:20+07:00"),
            Some(1_700_000_000_000)
        );
        assert_eq!(
            parse_iso8601_ms("2023-11-14T22:13:20.500Z"),
            Some(1_700_000_000_500)
        );
        // Spasi sebagai pemisah, offset tanpa titik dua.
        assert_eq!(
            parse_iso8601_ms("2023-11-15 05:13:20+0700"),
            Some(1_700_000_000_000)
        );
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_iso8601_ms("bukan tanggal"), None);
        assert_eq!(parse_iso8601_ms("2023/11/14 22:13:20"), None);
        assert_eq!(parse_iso8601_ms(""), None);
    }
}
