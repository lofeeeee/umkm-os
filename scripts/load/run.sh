#!/usr/bin/env bash
# P8-2: jalankan satu skenario k6 (PRD.md §13.4) sambil merekam sumber daya. Stack (API/worker/DB) harus sudah
# berjalan terhadap database uji beban (scripts/load/seed.sql + copy_website.sql). BUKAN untuk produksi.
#   scripts/load/run.sh <scenario: public|admin|api|chat> <out-dir> <tenants.json> [duration=60s]
# Hasil: <out>/<scenario>.summary.json (k6), <out>/<scenario>.resources.tsv (sampel 2 detik).
set -euo pipefail
SCEN=$1 OUT=$2 TEN=$3 DUR=${4:-60s}
PG=${PG_CONTAINER:-umkm-os-pg-test} REDIS=${REDIS_CONTAINER:-umkm-os-redis-test}
METRICS=${METRICS_URL:-http://127.0.0.1:9090/metrics} WMETRICS=${WORKER_METRICS_URL:-http://127.0.0.1:9091/metrics}
mkdir -p "$OUT"
res="$OUT/$SCEN.resources.tsv"
printf 'ts\tapi_cpu\tapi_rss_mb\tworker_cpu\tworker_rss_mb\tpg_cpu\tpg_mem\tredis_mem\tdb_conn_active\tpool_wait_count\tpool_wait_sum_s\tpool_wait_gt16ms\tqueue_depth\n' > "$res"
# CPU proses = selisih tick /proc/<pid>/stat antar-sampel (bukan rata-rata sejak start seperti `ps %cpu`).
ticks() { awk '{print $14 + $15}' "/proc/$1/stat" 2>/dev/null || echo 0; }
sample() {
  local hz api wrk pa pw ta=0 tw=0 t0 t1
  hz=$(getconf CLK_TCK)
  api=$(pgrep -x umkm-api | head -1) wrk=$(pgrep -x umkm-worker | head -1)
  t0=$(date +%s.%N); pa=$(ticks "$api"); pw=$(ticks "$wrk")
  while :; do
    sleep 2
    t1=$(date +%s.%N); ta=$(ticks "$api"); tw=$(ticks "$wrk")
    a=$(awk -v d="$((ta - pa))" -v s="$t0" -v e="$t1" -v hz="$hz" 'BEGIN{printf "%.1f", 100*d/hz/(e-s)}')
    w=$(awk -v d="$((tw - pw))" -v s="$t0" -v e="$t1" -v hz="$hz" 'BEGIN{printf "%.1f", 100*d/hz/(e-s)}')
    ar=$(awk '/VmRSS/{printf "%.0f", $2/1024}' "/proc/$api/status" 2>/dev/null)
    wr=$(awk '/VmRSS/{printf "%.0f", $2/1024}' "/proc/$wrk/status" 2>/dev/null)
    t0=$t1; pa=$ta; pw=$tw
    p=$(docker stats --no-stream --format '{{.CPUPerc}}\t{{.MemUsage}}' "$PG" | sed 's# / .*##')
    r=$(docker stats --no-stream --format '{{.MemUsage}}' "$REDIS" | sed 's# / .*##')
    m=$(curl -s "$METRICS" || true)
    db=$(printf '%s\n' "$m" | awk '/^db_connections_active /{print $2}')
    pool=$(printf '%s\n' "$m" | awk '/^db_pool_wait_seconds_count /{c=$2} /^db_pool_wait_seconds_sum /{s=$2} /^db_pool_wait_seconds_bucket\{le="0.016"\}/{b=$2} END{printf "%s\t%s\t%s", c, s, c-b}')
    q=$(curl -s "$WMETRICS" | awk '/^worker_queue_depth\{/{s+=$2} END{print s+0}')
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$(date +%s)" "$a" "${ar:-}" "$w" "${wr:-}" "$p" "$r" "${db:-}" "$pool" "${q:-}" >> "$res"
  done
}
sample & SAMPLER=$!
trap 'kill $SAMPLER 2>/dev/null || true' EXIT
docker run --rm --network host --user "$(id -u):$(id -g)" \
  -v "$(cd "$(dirname "$0")" && pwd):/scripts:ro" -v "$(dirname "$(realpath "$TEN")"):/data:ro" -v "$(realpath "$OUT"):/out" \
  -e SCENARIO="$SCEN" -e DURATION="$DUR" -e TENANTS="/data/$(basename "$TEN")" \
  grafana/k6:0.57.0 run --quiet --summary-export "/out/$SCEN.summary.json" /scripts/scenarios.js || echo "k6 exit $? (threshold gagal?)"
