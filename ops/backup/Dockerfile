# Image backup/restore: pg_dump/pg_restore 16 (sama dengan server pgvector:pg16) + age.
FROM postgres:16-alpine
RUN apk add --no-cache age=1.3.2-r1 tzdata \
 && adduser -D -u 10002 backup \
 && mkdir -p /backups && chown backup:backup /backups
COPY ops/backup/backup.sh ops/backup/restore.sh ops/backup/backup-loop.sh /usr/local/bin/
USER 10002:10002
ENTRYPOINT []
CMD ["backup-loop.sh"]
