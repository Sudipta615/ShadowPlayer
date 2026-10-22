#!/usr/bin/env bash
# Launch the real player on a headless Wayland compositor.
#
# The unit tests cannot prove that winit selects the Wayland backend, that the
# window is mapped, or that the app survives a compositor without XWayland.
# This script does: it starts weston's headless backend, routes ALSA to the null
# device (CI has no sound card), runs the release binary for a few seconds, and
# fails if the process exited on its own.
#
# Usage:
#   scripts/wayland-smoke.sh [path-to-shadowplayer]
set -euo pipefail

binary="${1:-target/release/shadowplayer}"
if [[ ! -x "$binary" ]]; then
    echo "error: $binary is not executable; build it first" >&2
    exit 1
fi
if ! command -v weston >/dev/null 2>&1; then
    echo "error: weston is required for the Wayland smoke test" >&2
    exit 1
fi

runtime="$(mktemp -d)"
chmod 700 "$runtime"
alsa_config="$runtime/asound.conf"
cat >"$alsa_config" <<'EOF'
pcm.!default {
    type null
}
EOF

socket="shadowplayer-smoke-$$"
weston_log="$runtime/weston.log"
app_log="$runtime/app.log"

cleanup() {
    if [[ -n "${app_pid:-}" ]] && kill -0 "$app_pid" 2>/dev/null; then
        kill -TERM "$app_pid" 2>/dev/null || true
        wait "$app_pid" 2>/dev/null || true
    fi
    if [[ -n "${weston_pid:-}" ]] && kill -0 "$weston_pid" 2>/dev/null; then
        kill -TERM "$weston_pid" 2>/dev/null || true
        wait "$weston_pid" 2>/dev/null || true
    fi
    rm -rf "$runtime"
}
trap cleanup EXIT

XDG_RUNTIME_DIR="$runtime" weston \
    --backend=headless-backend.so \
    --socket="$socket" \
    --width=1280 --height=800 \
    --log="$weston_log" &
weston_pid=$!

for _ in $(seq 1 100); do
    [[ -S "$runtime/$socket" ]] && break
    sleep 0.1
done
if [[ ! -S "$runtime/$socket" ]]; then
    echo "error: weston did not create its Wayland socket" >&2
    cat "$weston_log" >&2 || true
    exit 1
fi

XDG_RUNTIME_DIR="$runtime" \
WAYLAND_DISPLAY="$socket" \
LIBGL_ALWAYS_SOFTWARE=1 \
ALSA_CONFIG_PATH="$alsa_config" \
dbus-run-session -- "$binary" >"$app_log" 2>&1 &
app_pid=$!

# Startup includes database migrations and engine construction. Five seconds
# is long enough to catch a crash loop without making the job slow.
sleep 5
if ! kill -0 "$app_pid" 2>/dev/null; then
    echo "error: ShadowPlayer exited during the Wayland smoke test" >&2
    cat "$app_log" >&2 || true
    cat "$weston_log" >&2 || true
    exit 1
fi

if ! grep -q "Starting ShadowPlayer" "$app_log"; then
    echo "error: the app never reached its startup log" >&2
    cat "$app_log" >&2 || true
    exit 1
fi

echo "Wayland smoke test passed (pid $app_pid stayed alive for 5 s)"
