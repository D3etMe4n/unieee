#!/bin/sh
set -eu

: "${REDIS_PASSWORD:?REDIS_PASSWORD is required}"
: "${SENTINEL_ANNOUNCE_IP:?SENTINEL_ANNOUNCE_IP is required}"

sed \
  -e "s|\${REDIS_PASSWORD}|${REDIS_PASSWORD}|g" \
  -e "s|\${SENTINEL_ANNOUNCE_IP}|${SENTINEL_ANNOUNCE_IP}|g" \
  /etc/redis/sentinel.conf.tmpl > /tmp/sentinel.conf

exec redis-sentinel /tmp/sentinel.conf
