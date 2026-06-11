#!/bin/sh
# ki-browser smoke / resilience test harness
# ---------------------------------------------------------------------------
# Drives a running ki-browser instance over its REST API and asserts the CORE
# capabilities an AI agent needs in order to "drive" the browser. Every check
# does a REAL assertion on response *content* (not just HTTP 200) so a green
# run actually means the feature works end-to-end.
#
# Usage:
#   ./smoke.sh [BASE_URL]
#   BASE_URL defaults to http://192.168.50.65:9222
#
# Requires: POSIX sh, curl, jq.
#
# Exit code 0 = every check PASSed, 1 = at least one FAIL (or missing dep).
# ---------------------------------------------------------------------------

set -u

BASE="${1:-http://192.168.50.65:9222}"
BASE="${BASE%/}"   # strip a trailing slash so "$BASE/tabs" is always clean

# Per-request curl timeout (seconds). The popup-resilience check uses its own,
# deliberately tighter timeout to catch the 30s hang regression.
CURL_TIMEOUT="${CURL_TIMEOUT:-15}"
POPUP_TIMEOUT="${POPUP_TIMEOUT:-5}"

# --- pretty output ---------------------------------------------------------
if [ -t 1 ]; then
    C_GREEN="$(printf '\033[32m')"
    C_RED="$(printf '\033[31m')"
    C_YELLOW="$(printf '\033[33m')"
    C_BOLD="$(printf '\033[1m')"
    C_RESET="$(printf '\033[0m')"
else
    C_GREEN=""; C_RED=""; C_YELLOW=""; C_BOLD=""; C_RESET=""
fi

PASS_COUNT=0
FAIL_COUNT=0
# Names of failed checks, newline-separated, for the final summary.
FAILED_CHECKS=""

pass() {
    PASS_COUNT=$((PASS_COUNT + 1))
    printf '%s[ PASS ]%s %s\n' "$C_GREEN" "$C_RESET" "$1"
}

fail() {
    FAIL_COUNT=$((FAIL_COUNT + 1))
    FAILED_CHECKS="${FAILED_CHECKS}  - $1\n"
    printf '%s[ FAIL ]%s %s\n' "$C_RED" "$C_RESET" "$1"
    # Optional detail line.
    if [ $# -ge 2 ] && [ -n "$2" ]; then
        printf '         %s%s%s\n' "$C_YELLOW" "$2" "$C_RESET"
    fi
}

info() {
    printf '%s· %s%s\n' "$C_BOLD" "$1" "$C_RESET"
}

# Assert two values are equal; emits PASS/FAIL with the check name.
assert_eq() {
    # $1 = check name, $2 = expected, $3 = actual
    if [ "$2" = "$3" ]; then
        pass "$1"
    else
        fail "$1" "expected [$2] got [$3]"
    fi
}

# --- low-level HTTP helpers ------------------------------------------------
# All helpers print the raw response body to stdout. Network/timeout failures
# print the literal string __CURL_ERR__ so callers can distinguish a hang from
# a valid (even if error) JSON response.

http_get() {
    # $1 = path (with leading slash), $2 = optional timeout override
    _to="${2:-$CURL_TIMEOUT}"
    curl -s -m "$_to" "$BASE$1" 2>/dev/null || printf '__CURL_ERR__'
}

http_post() {
    # $1 = path, $2 = JSON body, $3 = optional timeout override
    _to="${3:-$CURL_TIMEOUT}"
    curl -s -m "$_to" -X POST \
        -H 'Content-Type: application/json' \
        -d "$2" \
        "$BASE$1" 2>/dev/null || printf '__CURL_ERR__'
}

http_delete() {
    # $1 = path, $2 = optional timeout override
    _to="${2:-$CURL_TIMEOUT}"
    curl -s -m "$_to" -X DELETE "$BASE$1" 2>/dev/null || printf '__CURL_ERR__'
}

# Convenience: open a tab with a given inline HTML document, echo its tab_id.
# $1 = HTML string. Uses jq to build the data: URL body safely (no nested shell
# quoting around the HTML — that was a real source of breakage).
open_html_tab() {
    _html="$1"
    _url="data:text/html,$( jq -nr --arg h "$_html" '$h|@uri' )"
    _body="$( jq -nc --arg u "$_url" '{url:$u, active:true}' )"
    _resp="$( http_post /tabs/new "$_body" )"
    printf '%s' "$_resp" | jq -r '.data.tab_id // empty' 2>/dev/null
}

# Evaluate a script in a given tab, echo the inner result value (JSON-encoded).
# The /evaluate envelope nests the value at .data.result.result (CDP wrapper +
# value); fall back to .data.result for any non-nested shape. Callers pipe the
# JSON output through `jq -r` to get the raw scalar.
# $1 = tab_id, $2 = script, $3 = optional timeout override
evaluate() {
    _body="$( jq -nc --arg t "$1" --arg s "$2" '{tab_id:$t, script:$s}' )"
    _resp="$( http_post /evaluate "$_body" "${3:-$CURL_TIMEOUT}" )"
    printf '%s' "$_resp" | jq -c '(.data.result.result // .data.result)' 2>/dev/null
}

close_tab() {
    [ -n "${1:-}" ] || return 0
    _body="$( jq -nc --arg t "$1" '{tab_id:$t}' )"
    http_post /tabs/close "$_body" >/dev/null 2>&1
}

# --- dependency check ------------------------------------------------------
for dep in curl jq; do
    if ! command -v "$dep" >/dev/null 2>&1; then
        printf '%s[ FAIL ]%s missing dependency: %s\n' "$C_RED" "$C_RESET" "$dep"
        exit 1
    fi
done

printf '%s=== ki-browser smoke test ===%s\n' "$C_BOLD" "$C_RESET"
printf 'Target: %s  (curl -m %ss, popup -m %ss)\n\n' "$BASE" "$CURL_TIMEOUT" "$POPUP_TIMEOUT"

# Track every tab/session we create so cleanup can run unconditionally.
CREATED_TABS=""
CREATED_SESSIONS=""
track_tab()     { [ -n "${1:-}" ] && CREATED_TABS="$CREATED_TABS $1"; }
track_session() { [ -n "${1:-}" ] && CREATED_SESSIONS="$CREATED_SESSIONS $1"; }

# ===========================================================================
# CHECK 1: /health reports healthy
# ===========================================================================
info "Check 1: GET /health"
HEALTH="$( http_get /health )"
HSTATUS="$( printf '%s' "$HEALTH" | jq -r '.data.status // .status // empty' 2>/dev/null )"
if [ "$HEALTH" = "__CURL_ERR__" ]; then
    fail "1. /health reachable" "no response within ${CURL_TIMEOUT}s"
elif printf '%s' "$HSTATUS" | grep -qi 'healthy'; then
    pass "1. /health -> status healthy ($HSTATUS)"
else
    fail "1. /health healthy" "status=[$HSTATUS] body=[$HEALTH]"
fi

# ===========================================================================
# CHECK 2: Tab lifecycle — create, list, title, close, gone
# ===========================================================================
info "Check 2: tab lifecycle (new -> list -> evaluate title -> close -> gone)"
LC_HTML='<!doctype html><html><head><title>SmokeLifecycle</title></head><body><h1 id="x">hello</h1></body></html>'
LC_TAB="$( open_html_tab "$LC_HTML" )"
track_tab "$LC_TAB"
if [ -z "$LC_TAB" ]; then
    fail "2a. /tabs/new returns tab_id"
else
    pass "2a. /tabs/new -> tab_id ($LC_TAB)"

    # 2b: GET /tabs contains the new tab
    TABS_JSON="$( http_get /tabs )"
    if printf '%s' "$TABS_JSON" | jq -e --arg t "$LC_TAB" \
        '.data.tabs[]? | select(.id == $t)' >/dev/null 2>&1; then
        pass "2b. /tabs contains the new tab"
    else
        fail "2b. /tabs contains the new tab" "tab $LC_TAB not in list"
    fi

    # 2c: document.title via /evaluate
    TITLE="$( evaluate "$LC_TAB" 'document.title' | jq -r '. // empty' 2>/dev/null )"
    assert_eq "2c. /evaluate document.title == SmokeLifecycle" "SmokeLifecycle" "$TITLE"

    # 2d: close the tab
    CLOSE_RESP="$( http_post /tabs/close "$( jq -nc --arg t "$LC_TAB" '{tab_id:$t}' )" )"
    if printf '%s' "$CLOSE_RESP" | jq -e '.success == true' >/dev/null 2>&1; then
        pass "2d. /tabs/close succeeds"
    else
        fail "2d. /tabs/close succeeds" "resp=[$CLOSE_RESP]"
    fi

    # 2e: tab is gone from /tabs
    TABS_AFTER="$( http_get /tabs )"
    if printf '%s' "$TABS_AFTER" | jq -e --arg t "$LC_TAB" \
        '.data.tabs[]? | select(.id == $t)' >/dev/null 2>&1; then
        fail "2e. closed tab no longer listed" "tab $LC_TAB still present"
    else
        pass "2e. closed tab no longer listed"
        CREATED_TABS="$( printf '%s' "$CREATED_TABS" | sed "s/ *$LC_TAB//" )"
    fi
fi

# ===========================================================================
# CHECK 3: Click — button onclick mutates document.title
# ===========================================================================
info "Check 3: /click triggers onclick"
CLICK_HTML='<!doctype html><html><head><title>before</title></head><body><button id="btn" onclick="document.title=42">go</button></body></html>'
CLICK_TAB="$( open_html_tab "$CLICK_HTML" )"
track_tab "$CLICK_TAB"
if [ -z "$CLICK_TAB" ]; then
    fail "3. click setup (tab create)"
else
    CLICK_BODY="$( jq -nc --arg t "$CLICK_TAB" '{tab_id:$t, selector:"#btn"}' )"
    CLICK_RESP="$( http_post /click "$CLICK_BODY" )"
    if printf '%s' "$CLICK_RESP" | jq -e '.success == true' >/dev/null 2>&1; then
        TITLE_AFTER="$( evaluate "$CLICK_TAB" 'String(document.title)' | jq -r '. // empty' 2>/dev/null )"
        assert_eq "3. /click -> onclick set document.title to 42" "42" "$TITLE_AFTER"
    else
        fail "3. /click succeeds" "resp=[$CLICK_RESP]"
    fi
fi

# ===========================================================================
# CHECK 4: /type into an input + value assert, and /scroll without error
# ===========================================================================
info "Check 4: /type + value assert, /scroll"
TYPE_HTML='<!doctype html><html><head><title>typetest</title></head><body><input id="inp" type="text"/><div style="height:5000px"></div></body></html>'
TYPE_TAB="$( open_html_tab "$TYPE_HTML" )"
track_tab "$TYPE_TAB"
if [ -z "$TYPE_TAB" ]; then
    fail "4. type/scroll setup (tab create)"
else
    TYPE_BODY="$( jq -nc --arg t "$TYPE_TAB" \
        '{tab_id:$t, selector:"#inp", text:"smoke-typed-123"}' )"
    TYPE_RESP="$( http_post /type "$TYPE_BODY" )"
    if printf '%s' "$TYPE_RESP" | jq -e '.success == true' >/dev/null 2>&1; then
        VAL="$( evaluate "$TYPE_TAB" 'document.getElementById("inp").value' | jq -r '. // empty' 2>/dev/null )"
        assert_eq "4a. /type -> input value" "smoke-typed-123" "$VAL"
    else
        fail "4a. /type succeeds" "resp=[$TYPE_RESP]"
    fi

    # 4b: /scroll should complete without error
    SCROLL_BODY="$( jq -nc --arg t "$TYPE_TAB" '{tab_id:$t, delta_y:800}' )"
    SCROLL_RESP="$( http_post /scroll "$SCROLL_BODY" )"
    if printf '%s' "$SCROLL_RESP" | jq -e '.success == true' >/dev/null 2>&1; then
        pass "4b. /scroll completes without error"
    else
        fail "4b. /scroll completes without error" "resp=[$SCROLL_RESP]"
    fi
fi

# ===========================================================================
# CHECK 5: Multi-tab + identity isolation
#   3 tabs created in parallel, each with an explicit, distinct timezone.
#   Assert each tab reports ITS OWN timezone (no cross-contamination).
# ===========================================================================
info "Check 5: multi-tab identity isolation (3 parallel tabs, distinct timezones)"

ISO_TMP="$( mktemp -d 2>/dev/null || printf '/tmp/smoke.%s' "$$" )"
mkdir -p "$ISO_TMP" 2>/dev/null || true

# Tab N gets timezone TZ_N. Distinct IANA zones with distinct offsets.
TZ_1="Europe/Berlin"
TZ_2="America/New_York"
TZ_3="Asia/Tokyo"

create_identity_tab() {
    # $1 = timezone, $2 = output file for tab_id
    _idbody="$( jq -nc --arg tz "$1" \
        '{url:"about:blank", active:false, identity:{timezone:$tz}}' )"
    _resp="$( http_post /tabs/new "$_idbody" )"
    printf '%s' "$_resp" | jq -r '.data.tab_id // empty' 2>/dev/null > "$2"
}

# Launch all three in parallel.
create_identity_tab "$TZ_1" "$ISO_TMP/t1" &
P1=$!
create_identity_tab "$TZ_2" "$ISO_TMP/t2" &
P2=$!
create_identity_tab "$TZ_3" "$ISO_TMP/t3" &
P3=$!
wait "$P1" 2>/dev/null || true
wait "$P2" 2>/dev/null || true
wait "$P3" 2>/dev/null || true

IT1="$( cat "$ISO_TMP/t1" 2>/dev/null )"
IT2="$( cat "$ISO_TMP/t2" 2>/dev/null )"
IT3="$( cat "$ISO_TMP/t3" 2>/dev/null )"
track_tab "$IT1"; track_tab "$IT2"; track_tab "$IT3"

if [ -z "$IT1" ] || [ -z "$IT2" ] || [ -z "$IT3" ]; then
    fail "5a. 3 parallel /tabs/new with identity" "ids=[$IT1|$IT2|$IT3]"
else
    pass "5a. 3 parallel /tabs/new with identity created"

    # Per-tab: GET /tabs/{id}/identity and assert the explicit timezone matches.
    get_tz() { http_get "/tabs/$1/identity" | jq -r '.data.timezone // empty' 2>/dev/null; }
    RTZ1="$( get_tz "$IT1" )"
    RTZ2="$( get_tz "$IT2" )"
    RTZ3="$( get_tz "$IT3" )"

    assert_eq "5b. tab1 timezone == $TZ_1" "$TZ_1" "$RTZ1"
    assert_eq "5c. tab2 timezone == $TZ_2" "$TZ_2" "$RTZ2"
    assert_eq "5d. tab3 timezone == $TZ_3" "$TZ_3" "$RTZ3"

    # Cross-contamination guard: the three reported zones must be 3 distinct
    # values (proves identities are not shared/crossed between tabs).
    DISTINCT="$( printf '%s\n%s\n%s\n' "$RTZ1" "$RTZ2" "$RTZ3" | sort -u | grep -c . )"
    assert_eq "5e. timezones not crossed (3 distinct)" "3" "$DISTINCT"
fi
rm -rf "$ISO_TMP" 2>/dev/null || true

# ===========================================================================
# CHECK 6: POPUP RESILIENCE  (the critical hang regression)
#   Open a tab, trigger window.open(...,"_blank"), then immediately confirm the
#   browser stays RESPONSIVE: a /tabs call AND a /evaluate must each answer
#   within POPUP_TIMEOUT seconds. If either hangs -> FAIL "popup hang".
# ===========================================================================
info "Check 6: popup resilience (window.open must not hang the browser)"
POPUP_HTML='<!doctype html><html><head><title>popup-host</title></head><body>host</body></html>'
POPUP_TAB="$( open_html_tab "$POPUP_HTML" )"
track_tab "$POPUP_TAB"
if [ -z "$POPUP_TAB" ]; then
    fail "6. popup setup (tab create)"
else
    # Fire the popup. We do NOT assert on this call's own success — opening a
    # popup may legitimately return oddly; what matters is responsiveness AFTER.
    # Give it the normal timeout so the trigger itself isn't what we measure.
    POPUP_EVAL_BODY="$( jq -nc --arg t "$POPUP_TAB" \
        '{tab_id:$t, script:"window.open(\"https://example.com\",\"_blank\"); 1", await_promise:false}' )"
    http_post /evaluate "$POPUP_EVAL_BODY" >/dev/null 2>&1

    # Immediately probe responsiveness with a TIGHT timeout.
    T_START="$( date +%s )"
    TABS_PROBE="$( http_get /tabs "$POPUP_TIMEOUT" )"
    if [ "$TABS_PROBE" = "__CURL_ERR__" ]; then
        fail "6a. /tabs responsive after popup" "popup hang: no answer within ${POPUP_TIMEOUT}s"
    elif printf '%s' "$TABS_PROBE" | jq -e '.success == true' >/dev/null 2>&1; then
        pass "6a. /tabs responsive within ${POPUP_TIMEOUT}s after popup"
    else
        fail "6a. /tabs responsive after popup" "unexpected body=[$TABS_PROBE]"
    fi

    TITLE_PROBE="$( evaluate "$POPUP_TAB" 'document.title' "$POPUP_TIMEOUT" )"
    if [ -z "$TITLE_PROBE" ] || [ "$TITLE_PROBE" = "__CURL_ERR__" ]; then
        # Distinguish a hang (curl error / empty) from a wrong value.
        RAW="$( http_post /evaluate "$( jq -nc --arg t "$POPUP_TAB" '{tab_id:$t, script:"document.title"}' )" "$POPUP_TIMEOUT" )"
        if [ "$RAW" = "__CURL_ERR__" ]; then
            fail "6b. /evaluate responsive after popup" "popup hang: no answer within ${POPUP_TIMEOUT}s"
        else
            fail "6b. /evaluate responsive after popup" "empty result, body=[$RAW]"
        fi
    else
        pass "6b. /evaluate responsive within ${POPUP_TIMEOUT}s after popup"
    fi
    T_END="$( date +%s )"
    info "   (popup responsiveness probes took $((T_END - T_START))s total)"
fi

# ===========================================================================
# CHECK 7: Session inheritance
#   Import a minimal bundle (origin example.com, 1 visible cookie + 1 localStorage
#   entry) -> session_id. Confirm it appears in /login-session/list. Create a tab
#   from it, then assert the cookie + localStorage are present in that tab.
#   Finally DELETE the session.
# ===========================================================================
info "Check 7: session inheritance (import -> list -> tab inherits cookie+localStorage -> delete)"

SESS_ORIGIN="https://example.com"
SESS_BUNDLE="$( jq -nc --arg origin "$SESS_ORIGIN" '
    {
      version: 1,
      origin: $origin,
      cookies: [
        { name: "smoke_cookie", value: "smoke_val_777",
          domain: ".example.com", path: "/", secure: true, httpOnly: false }
      ],
      storage: [
        { origin: $origin,
          local: { smoke_ls: "ls_val_888" },
          session: {} }
      ]
    }' )"

IMPORT_RESP="$( http_post /login-session/import "$SESS_BUNDLE" )"
SESSION_ID="$( printf '%s' "$IMPORT_RESP" | jq -r '.data.session_id // empty' 2>/dev/null )"
track_session "$SESSION_ID"

if [ -z "$SESSION_ID" ]; then
    fail "7a. /login-session/import -> session_id" "resp=[$IMPORT_RESP]"
else
    pass "7a. /login-session/import -> session_id ($SESSION_ID)"

    # 7b: appears in list
    LIST_RESP="$( http_get /login-session/list )"
    if printf '%s' "$LIST_RESP" | jq -e --arg id "$SESSION_ID" \
        '.data[]? | select(.id == $id)' >/dev/null 2>&1; then
        pass "7b. /login-session/list contains the session"
    else
        fail "7b. /login-session/list contains the session" "resp=[$LIST_RESP]"
    fi

    # 7c: create a tab from the session and inherit state.
    # Navigate to the bundle origin so cookies/localStorage apply to that document.
    SESS_TAB_BODY="$( jq -nc --arg id "$SESSION_ID" --arg u "$SESS_ORIGIN/" \
        '{url:$u, active:true, session_id:$id}' )"
    SESS_TAB_RESP="$( http_post /tabs/new "$SESS_TAB_BODY" )"
    SESS_TAB="$( printf '%s' "$SESS_TAB_RESP" | jq -r '.data.tab_id // empty' 2>/dev/null )"
    track_tab "$SESS_TAB"

    if [ -z "$SESS_TAB" ]; then
        fail "7c. /tabs/new with session_id -> tab" "resp=[$SESS_TAB_RESP]"
    else
        pass "7c. /tabs/new with session_id -> tab ($SESS_TAB)"

        # Give the page a moment to settle the document/origin before reading.
        # (No foreground sleep dependency on the harness — short curl-bound wait.)
        i=0
        while [ "$i" -lt 10 ]; do
            READY="$( evaluate "$SESS_TAB" 'String(location.host || "")' | jq -r '. // empty' 2>/dev/null )"
            case "$READY" in
                *example.com*) break ;;
            esac
            i=$((i + 1))
            # tiny network-bound delay: a no-op evaluate acts as a pacing call
            evaluate "$SESS_TAB" '1' >/dev/null 2>&1
        done

        # 7d: cookie present in the tab's document.cookie
        COOKIE_VAL="$( evaluate "$SESS_TAB" \
            '(document.cookie.match(/(?:^|; )smoke_cookie=([^;]*)/)||[])[1]||""' \
            | jq -r '. // empty' 2>/dev/null )"
        assert_eq "7d. inherited cookie visible in tab" "smoke_val_777" "$COOKIE_VAL"

        # 7e: localStorage entry present
        LS_VAL="$( evaluate "$SESS_TAB" 'localStorage.getItem("smoke_ls")||""' \
            | jq -r '. // empty' 2>/dev/null )"
        assert_eq "7e. inherited localStorage visible in tab" "ls_val_888" "$LS_VAL"
    fi

    # 7f: delete the session
    DEL_RESP="$( http_delete "/login-session/$SESSION_ID" )"
    if printf '%s' "$DEL_RESP" | jq -e '.success == true' >/dev/null 2>&1; then
        pass "7f. DELETE /login-session/{id} succeeds"
        CREATED_SESSIONS="$( printf '%s' "$CREATED_SESSIONS" | sed "s/ *$SESSION_ID//" )"
    else
        fail "7f. DELETE /login-session/{id} succeeds" "resp=[$DEL_RESP]"
    fi
fi

# ===========================================================================
# CHECK 8: Cleanup — close every test tab, delete every test session.
#   This is best-effort and not asserted as PASS/FAIL itself, but we report it.
# ===========================================================================
info "Check 8: cleanup (close test tabs, delete test sessions)"
for t in $CREATED_TABS; do
    close_tab "$t"
done
for s in $CREATED_SESSIONS; do
    http_delete "/login-session/$s" >/dev/null 2>&1
done
pass "8. cleanup completed (closed test tabs, removed test sessions)"

# ===========================================================================
# SUMMARY
# ===========================================================================
printf '\n%s=== Summary ===%s\n' "$C_BOLD" "$C_RESET"
printf '%sPASS: %d%s   %sFAIL: %d%s\n' \
    "$C_GREEN" "$PASS_COUNT" "$C_RESET" "$C_RED" "$FAIL_COUNT" "$C_RESET"

if [ "$FAIL_COUNT" -gt 0 ]; then
    printf '%sFailed checks:%s\n' "$C_RED" "$C_RESET"
    printf "$FAILED_CHECKS"
    printf '\n%sRESULT: FAIL%s\n' "$C_RED" "$C_RESET"
    exit 1
fi

printf '\n%sRESULT: ALL GREEN%s\n' "$C_GREEN" "$C_RESET"
exit 0
