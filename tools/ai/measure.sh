#!/bin/zsh
# M10: runs the models end to end on photos (src-tauri/src/ai/tests.rs) and writes the numbers.
#   tools/ai/measure.sh <model files dir> <photos dir> <report.json> [samples dir]
# The model files are served from a local folder, so the app's real install path runs (download,
# SHA-256 check, GPU rewrite) without the network.
set -e
cd "${0:A:h}/../.."
export LAWHA_MODEL_DIR="${1:A}" LAWHA_TEST_PHOTOS="${2:A}" LAWHA_AI_REPORT="${3:A}"
[ -n "$4" ] && { mkdir -p "$4"; export LAWHA_AI_SAMPLES="${4:A}"; }
nice cargo test -p lawha --lib ai::tests::models_on_photos -- --ignored --nocapture
