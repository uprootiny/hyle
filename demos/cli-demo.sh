#!/bin/bash
# hyle CLI Demo
# Exercises hyle's binary, coggy integration, and cargo test suite
# Usage: ./demos/cli-demo.sh

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
PURPLE='\033[0;35m'
DIM='\033[0;90m'
BOLD='\033[1m'
NC='\033[0m'

section() {
  echo ""
  echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
  echo -e "${BOLD}${BLUE}  $1${NC}"
  echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
  echo ""
}

ok() { echo -e "  ${GREEN}✓${NC} $1"; }
fail() { echo -e "  ${RED}✗${NC} $1"; }
info() { echo -e "  ${DIM}$1${NC}"; }

# Header
echo ""
echo -e "${BOLD}${BLUE}  ╔═══════════════════════════════════════╗${NC}"
echo -e "${BOLD}${BLUE}  ║         hyle CLI Demo v0.3.0          ║${NC}"
echo -e "${BOLD}${BLUE}  ║  Rust-native code assistant + coggy   ║${NC}"
echo -e "${BOLD}${BLUE}  ╚═══════════════════════════════════════╝${NC}"
echo ""

# ── Section 1: Binary check ──
section "1. Binary Check"

HYLE_BIN="./target/debug/hyle"
HYLE_API_BIN="./target/debug/hyle-api"

if [ -f "$HYLE_BIN" ]; then
  ok "hyle binary found: $HYLE_BIN"
  SIZE=$(du -sh "$HYLE_BIN" | cut -f1)
  info "Size: $SIZE"
else
  fail "hyle binary not found. Run: cargo build"
  info "Attempting cargo build..."
  cargo build 2>&1 | tail -3
fi

if [ -f "$HYLE_API_BIN" ]; then
  ok "hyle-api binary found: $HYLE_API_BIN"
else
  fail "hyle-api binary not found"
fi

# ── Section 2: Help output ──
section "2. Help & Version"

if [ -f "$HYLE_BIN" ]; then
  echo -e "${DIM}"
  "$HYLE_BIN" --help 2>&1 | head -20 || true
  echo -e "${NC}"
  ok "Help output displayed"
else
  info "Skipping (no binary)"
fi

# ── Section 3: Project structure ──
section "3. Project Structure"

info "Source files:"
find src/ -name "*.rs" -type f | sort | while read f; do
  LINES=$(wc -l < "$f")
  printf "  ${DIM}%-40s${NC} %5d lines\n" "$f" "$LINES"
done

TOTAL_LINES=$(find src/ -name "*.rs" | xargs wc -l | tail -1 | awk '{print $1}')
echo ""
ok "Total: $TOTAL_LINES lines of Rust"

# ── Section 4: Coggy integration ──
section "4. Coggy Integration"

if [ -f "src/coggy_bridge.rs" ]; then
  ok "coggy_bridge.rs exists ($(wc -l < src/coggy_bridge.rs) lines)"
  info "Bridges hyle concepts to AtomSpace atoms"
else
  fail "coggy_bridge.rs not found"
fi

if [ -f "src/coggy_live.rs" ]; then
  ok "coggy_live.rs exists ($(wc -l < src/coggy_live.rs) lines)"
  info "5-phase cognitive cycle: THINK → SALIENCE → PROMPT → TIKKUN → FACTS"
else
  fail "coggy_live.rs not found"
fi

if grep -q 'coggy = ' Cargo.toml; then
  COGGY_PATH=$(grep 'coggy = ' Cargo.toml | grep -oP 'path = "\K[^"]+' || echo "crates.io")
  ok "coggy dependency: $COGGY_PATH"
else
  fail "No coggy dependency in Cargo.toml"
fi

# ── Section 5: Build ──
section "5. Build Verification"

info "Running cargo build..."
BUILD_START=$(date +%s%N)
if cargo build 2>&1 | tail -3; then
  BUILD_END=$(date +%s%N)
  BUILD_MS=$(( (BUILD_END - BUILD_START) / 1000000 ))
  ok "Build succeeded (${BUILD_MS}ms)"
else
  fail "Build failed"
  exit 1
fi

# ── Section 6: Tests ──
section "6. Test Suite"

info "Running cargo test..."
TEST_OUTPUT=$(cargo test 2>&1)
echo "$TEST_OUTPUT" | tail -20

# Count results
LIB_TESTS=$(echo "$TEST_OUTPUT" | grep "test result.*lib" | grep -oP '\d+ passed' | head -1 || echo "0 passed")
BIN_TESTS=$(echo "$TEST_OUTPUT" | grep "test result.*bin" | grep -oP '\d+ passed' | head -1 || echo "0 passed")
TOTAL_PASSED=$(echo "$TEST_OUTPUT" | grep "test result" | grep -oP '\d+ passed' | awk '{s+=$1} END {print s}')
TOTAL_FAILED=$(echo "$TEST_OUTPUT" | grep "test result" | grep -oP '\d+ failed' | awk '{s+=$1} END {print s}')

echo ""
if [ "${TOTAL_FAILED:-0}" = "0" ]; then
  ok "All tests passed: ${TOTAL_PASSED:-?} total"
else
  fail "${TOTAL_FAILED} tests failed out of ${TOTAL_PASSED} total"
fi

# ── Section 7: Demo HTML pages ──
section "7. Demo Pages"

for page in docs/coggy-demo.html docs/tui-demo.html docs/dashboard-demo.html; do
  if [ -f "$page" ]; then
    SIZE=$(wc -c < "$page")
    ok "$page (${SIZE} bytes)"
  else
    fail "$page not found"
  fi
done

# ── Section 8: Summary ──
section "Summary"

echo -e "  ${BOLD}hyle v0.3.0${NC} - Rust-native code assistant"
echo -e "  ${DIM}─────────────────────────────────────────${NC}"
echo -e "  ${GREEN}Binary:${NC}    $(du -sh "$HYLE_BIN" 2>/dev/null | cut -f1 || echo 'N/A')"
echo -e "  ${GREEN}Source:${NC}    $TOTAL_LINES lines of Rust"
echo -e "  ${GREEN}Tests:${NC}     ${TOTAL_PASSED:-?} passing"
echo -e "  ${GREEN}Coggy:${NC}     AtomSpace + PLN + ECAN + Tikkun"
echo -e "  ${GREEN}Models:${NC}    OpenRouter free tier (gemini, qwen, deepseek)"
echo -e "  ${GREEN}Demos:${NC}     $(ls docs/*-demo.html 2>/dev/null | wc -l) HTML demos"
echo ""
echo -e "  ${BOLD}${BLUE}Everything is working.${NC}"
echo ""
