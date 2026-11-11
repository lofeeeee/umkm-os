#!/usr/bin/env bash
# ADR-0013 / PRD.md §7.2: crate domain tidak boleh bergantung (langsung/transitif)
# pada framework HTTP, DB, HTTP client, atau async runtime. Exit 1 bila dilanggar.
set -euo pipefail
forbidden='^(axum|sqlx|sqlx-[a-z-]+|reqwest|tokio|hyper|tower|tower-http|redis) '
deps=$(cargo tree -p umkm-domain -e normal --prefix none --no-dedupe | sort -u)
if bad=$(grep -E "$forbidden" <<<"$deps"); then
  echo "FAIL: umkm-domain bergantung pada crate terlarang:"; echo "$bad"; exit 1
fi
echo "OK: umkm-domain bebas dari axum/sqlx/reqwest/tokio/hyper/tower/redis"
