#!/usr/bin/env bash

if [ -t 1 ]; then
    _C_RED='\033[0;31m'
    _C_GREEN='\033[0;32m'
    _C_YELLOW='\033[0;33m'
    _C_BLUE='\033[0;34m'
    _C_RESET='\033[0m'
else
    _C_RED=''
    _C_GREEN=''
    _C_YELLOW=''
    _C_BLUE=''
    _C_RESET=''
fi

log_info() {
    printf "%b[INFO]%b %s\n" "$_C_BLUE" "$_C_RESET" "$*"
}

log_ok() {
    printf "%b[OK]%b   %s\n" "$_C_GREEN" "$_C_RESET" "$*"
}

log_warn() {
    printf "%b[WARN]%b %s\n" "$_C_YELLOW" "$_C_RESET" "$*" >&2
}

log_err() {
    printf "%b[ERR]%b  %s\n" "$_C_RED" "$_C_RESET" "$*" >&2
}

load_env_file() {
    local root_dir="$1"
    local env_file="$root_dir/.env"
    local example_file="$root_dir/.env.example"

    if [ ! -f "$env_file" ] && [ ! -f "$example_file" ]; then
        log_err "Neither .env nor .env.example found in $root_dir"
        return 1
    fi

    if [ -f "$example_file" ]; then
        set -a
        # shellcheck disable=SC1090
        . "$example_file"
        set +a
        ENV_FILE_ARG="$example_file"
    fi

    if [ -f "$env_file" ]; then
        set -a
        # shellcheck disable=SC1090
        . "$env_file"
        set +a
        ENV_FILE_ARG="$env_file"
    else
        log_warn ".env not found; falling back to .env.example (dev defaults)"
    fi
}
