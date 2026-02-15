#!/bin/bash
# Performance benchmarks for jaq-py
# Usage: RUNS=5 ./bench.sh
cd "$(dirname "$0")"

RUNS=${RUNS:-3}

bench() {
    local name=$1
    shift
    local total_time=0 total_mem=0

    for _ in $(seq 1 $RUNS); do
        out=$(/usr/bin/time -l "$@" 2>&1 >/dev/null)
        t=$(echo "$out" | awk '/real/{print $1}')
        m=$(echo "$out" | awk '/maximum resident set size/{print $1}')
        total_time=$(echo "$total_time + $t" | bc)
        total_mem=$(echo "$total_mem + $m" | bc)
    done

    avg_t=$(echo "scale=2; $total_time / $RUNS" | bc)
    avg_m=$(echo "scale=0; $total_mem / 1024 / 1024 / $RUNS" | bc)
    printf "  %-45s %6ss  %6s MB\n" "$name" "$avg_t" "$avg_m"
}

run_benchmark() {
    local data=$1
    local filter=$2
    local iterations=$3

    echo "Data: $data ($(du -h "$data" | cut -f1))  |  Iterations: $iterations  |  Runs: $RUNS"
    echo ""

    echo "CLI:"
    bench "jq" jq "$filter" "$data"
    bench "jaq" "$HOME/.cargo/bin/jaq" "$filter" "$data"

    echo ""
    echo "Python + subprocess:"
    bench "jq + subprocess" uv run python scripts/jq_subprocess.py "$data" "$filter" "$iterations"
    bench "jaq + subprocess" uv run python scripts/jaq_subprocess.py "$data" "$filter" "$iterations"

    echo ""
    echo "Python bindings:"
    bench "jq-python (input/output as Python objects)" uv run python scripts/jq_bindings_pyobj.py "$data" "$filter" "$iterations"
    bench "jaq-py (input/output as Python objects)" uv run python scripts/jaq_bindings_pyobj.py "$data" "$filter" "$iterations"
    bench "jaq-py (input/output as text)" uv run python scripts/jaq_bindings_text.py "$data" "$filter" "$iterations"
}

main() {
    echo "Building jaq-py in release mode..."
    (cd .. && uv run maturin develop --release 2>&1 | grep -E "^(error|warning)" || true)
    echo ""

    echo "=== Large File (53 MB, 1 iteration) ==="
    echo ""
    run_benchmark "data/large.json" "$(cat data/large.jq)" 1

    echo ""
    echo "=== Many Invocations (1000x) ==="
    echo ""
    run_benchmark "data/small.json" "$(cat data/small.jq)" 1000
}

main
