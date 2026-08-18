#!/usr/bin/env bash
set -euo pipefail

repository=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
collector_image='ghcr.io/open-telemetry/opentelemetry-collector-releases/opentelemetry-collector:0.157.0@sha256:4019ce4d7e7791a1a255fffb2f407af66d5017cc65543469ba565c4f47f795b8'
results=${PAM_OTLP_EVIDENCE_DIR:-"${repository}/artifacts/otlp"}
container="pam-otel-cert-${$}"
target_created=0
image_present=0
server_pid=''

if docker image inspect "${collector_image}" >/dev/null 2>&1; then
    image_present=1
fi

cleanup() {
    if [[ -n "${server_pid}" ]]; then
        kill "${server_pid}" >/dev/null 2>&1 || true
        wait "${server_pid}" >/dev/null 2>&1 || true
    fi
    docker container rm --force "${container}" >/dev/null 2>&1 || true
    if [[ "${image_present}" -eq 0 ]]; then
        docker image rm "${collector_image}" >/dev/null 2>&1 || true
    fi
    if [[ "${target_created}" -eq 1 ]]; then
        cargo clean --target-dir "${target_dir}" >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT INT TERM

if [[ -L "${results}" ]]; then
    printf 'OTLP evidence directory must not be a symlink: %s\n' "${results}" >&2
    exit 1
fi
mkdir -p "${results}"
for artifact in collector.log metadata.json pam.stderr.log report.json evidence-manifest.json; do
    artifact_path="${results}/${artifact}"
    if [[ -L "${artifact_path}" || ( -e "${artifact_path}" && ! -f "${artifact_path}" ) ]]; then
        printf 'OTLP evidence artifact path is unsafe: %s\n' "${artifact_path}" >&2
        exit 1
    fi
    find "${artifact_path}" -maxdepth 0 -type f -delete 2>/dev/null || true
done

if [[ -n "${PAM_BIN:-}" ]]; then
    pam_bin=${PAM_BIN}
else
    target_dir=$(mktemp -d /tmp/pam-otel-target.XXXXXX)
    target_created=1
    CARGO_TARGET_DIR="${target_dir}" cargo build --manifest-path "${repository}/Cargo.toml" --locked
    pam_bin="${target_dir}/debug/pam"
fi

docker run --detach --name "${container}" \
    --cap-drop ALL \
    --read-only \
    --security-opt no-new-privileges=true \
    --publish 127.0.0.1::4318 \
    --volume "${repository}/tests/fixtures/otel-collector.yaml:/etc/otelcol/config.yaml:ro" \
    "${collector_image}" --config=/etc/otelcol/config.yaml >/dev/null
collector_port=$(docker port "${container}" 4318/tcp | awk -F: 'NR == 1 { print $NF }')
if [[ ! "${collector_port}" =~ ^[0-9]+$ ]]; then
    printf 'collector did not publish a valid OTLP port\n' >&2
    exit 1
fi
collector_ready=0
for _ in $(seq 1 100); do
    status=$(curl --silent --output /dev/null --write-out '%{http_code}' --max-time 1 \
        --header 'content-type: application/json' \
        --data '{"resourceSpans":[]}' \
        "http://127.0.0.1:${collector_port}/v1/traces" || true)
    if [[ "${status}" == '200' ]]; then
        collector_ready=1
        break
    fi
    if ! docker container inspect "${container}" --format '{{.State.Running}}' 2>/dev/null | grep -Fxq true; then
        docker logs "${container}" >&2 || true
        printf 'OpenTelemetry Collector stopped before becoming ready\n' >&2
        exit 1
    fi
    sleep 0.05
done
if [[ "${collector_ready}" -ne 1 ]]; then
    docker logs "${container}" >&2 || true
    printf 'OpenTelemetry Collector did not become ready\n' >&2
    exit 1
fi

server_port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')
OTEL_SERVICE_NAME=pam-certification \
OTEL_EXPORTER_OTLP_TRACES_ENDPOINT="http://127.0.0.1:${collector_port}/v1/traces" \
OTEL_EXPORTER_OTLP_TRACES_PROTOCOL=http/json \
OTEL_BSP_MAX_EXPORT_BATCH_SIZE=1 \
OTEL_BSP_SCHEDULE_DELAY=1 \
PAM_TEST_PORT="${server_port}" \
"${pam_bin}" "${repository}/tests/fixtures/server.php" \
    >/dev/null 2>"${results}/pam.stderr.log" &
server_pid=$!

ready=0
for _ in $(seq 1 100); do
    if curl --silent --fail --max-time 1 "http://127.0.0.1:${server_port}/metrics" >/dev/null 2>&1; then
        ready=1
        break
    fi
    if ! kill -0 "${server_pid}" >/dev/null 2>&1; then
        printf 'PAM stopped before OTLP certification\n' >&2
        exit 1
    fi
    sleep 0.05
done
if [[ "${ready}" -ne 1 ]]; then
    printf 'PAM did not become ready for OTLP certification\n' >&2
    exit 1
fi

trace_id='4bf92f3577b34da6a3ce929d0e0e4736'
parent_span_id='00f067aa0ba902b7'
curl --silent --fail --max-time 5 \
    --header "traceparent: 00-${trace_id}-${parent_span_id}-01" \
    "http://127.0.0.1:${server_port}/ping?secret=must-not-leak" >/dev/null

accepted=0
for _ in $(seq 1 100); do
    docker logs "${container}" >"${results}/collector.log" 2>&1
    if grep -Fq "${trace_id}" "${results}/collector.log"; then
        accepted=1
        break
    fi
    sleep 0.05
done

route_redacted=1
if grep -Fq 'must-not-leak' "${results}/collector.log"; then
    route_redacted=0
fi
parent_preserved=0
if grep -Fq "${parent_span_id}" "${results}/collector.log"; then
    parent_preserved=1
fi
service_identified=0
if grep -Fq 'pam-certification' "${results}/collector.log"; then
    service_identified=1
fi
passed=0
if [[ "${accepted}" -eq 1 && "${route_redacted}" -eq 1 && "${parent_preserved}" -eq 1 && "${service_identified}" -eq 1 ]]; then
    passed=1
fi

source_commit=$(git -C "${repository}" rev-parse HEAD)
source_dirty=false
if [[ -n "$(git -C "${repository}" status --porcelain --untracked-files=no)" ]]; then
    source_dirty=true
fi
python3 - "${results}/metadata.json" "${source_commit}" "${source_dirty}" "${collector_image}" <<'PY'
import json, sys
path, commit, dirty, collector = sys.argv[1:]
value = {
    "schema_version": 1,
    "suite_id": 1,
    "source": {"commit": commit, "dirty": dirty == "true"},
    "collector": {"distribution": "core", "image": collector},
    "protocol": "http/json",
}
open(path, "w", encoding="utf-8").write(json.dumps(value, indent=2, sort_keys=True) + "\n")
PY
python3 - "${results}/report.json" "${passed}" "${accepted}" "${parent_preserved}" "${service_identified}" "${route_redacted}" <<'PY'
import json, sys
path, passed, accepted, parent, service, redacted = sys.argv[1:]
value = {
    "schema_version": 1,
    "suite_id": 1,
    "passed": passed == "1",
    "gates": {
        "collector_accepted": accepted == "1",
        "parent_span_preserved": parent == "1",
        "service_identified": service == "1",
        "sensitive_query_redacted": redacted == "1",
    },
}
open(path, "w", encoding="utf-8").write(json.dumps(value, indent=2, sort_keys=True) + "\n")
PY
if [[ "${passed}" -ne 1 ]]; then
    printf 'OTLP collector certification failed; inspect %s\n' "${results}" >&2
    exit 1
fi
python3 "${repository}/scripts/otlp-evidence.py" "${results}" 1
python3 "${repository}/scripts/otlp-evidence.py" "${results}" 1 --verify
printf 'PAM OTLP collector certification passed: %s\n' "${results}"
