#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
mkdir -p ./logs
exec gnss-sdr --config_file=./rtl-sdr-gnss.conf --log_dir=./logs
