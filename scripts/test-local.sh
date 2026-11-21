#!/usr/bin/env bash
set -euo pipefail

cargo build --release
./target/release/paper-bumper check --project-dir "${1:-.}"
