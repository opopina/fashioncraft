#!/bin/sh
# Starts a FashionCraft AppImage the way the AppImage catalog's test does (appimage.github.io:
# Ubuntu 22.04, Xvfb with Mesa's software OpenGL, no network) and fails unless its window
# appears and it stays up.
#
#   check-appimage.sh FILE.AppImage [SECONDS] [SCREENSHOT.png]
#
# Run it inside an X server (xvfb-run) and without network (unshare --net, docker --network
# none): FashionCraft must start offline. Needs xwininfo (x11-utils), and ImageMagick's import for
# the screenshot. Without FUSE (a container), set APPIMAGE_EXTRACT_AND_RUN=1; the catalog does not
# use the AppImage's FUSE mount either (firejail --appimage mounts it).
#
# Fails when the program exits before SECONDS (30), when no FashionCraft window is on screen by
# then, or when its output says a library is missing ("could not be loaded": a dlopen; "error
# while loading shared libraries": a NEEDED one), a panic, or an error before the window
# ("FashionCraft: ...").
set -u
appimage=$1
seconds=${2:-30}
screenshot=${3:-}

# /proc/net is this process's network namespace (/sys/class/net would show the host's).
echo "Network interfaces: $(awk -F: 'NR > 2 { gsub(/ /, "", $1); printf "%s ", $1 }' /proc/net/dev)"
if ! xwininfo -root >/dev/null; then
    echo "ERROR: xwininfo cannot reach the X server (DISPLAY=${DISPLAY:-unset})"
    exit 1
fi

# A FashionCraft window (by title or by its X11 class, the app id) that is mapped and viewable.
viewable_window() {
    xwininfo -root -tree 2>/dev/null | grep -i fashioncraft | awk '{print $1}' | while read -r id; do
        if xwininfo -id "$id" 2>/dev/null | grep -q IsViewable; then
            echo "$id"
            break
        fi
    done
}

log=$(mktemp)
chmod +x "$appimage"
"$appimage" >"$log" 2>&1 &
pid=$!

window=""
waited=0
while [ "$waited" -lt "$seconds" ] && kill -0 "$pid" 2>/dev/null; do
    sleep 1
    waited=$((waited + 1))
    if [ -z "$window" ] && [ -n "$(viewable_window)" ]; then
        window=$waited
        echo "FashionCraft's window is on screen after $waited s"
    fi
done

alive=no
kill -0 "$pid" 2>/dev/null && alive=yes
if [ -n "$screenshot" ] && [ "$alive" = yes ] && command -v import >/dev/null; then
    import -window root "$screenshot" || true
fi
kill "$pid" 2>/dev/null
sleep 2
kill -9 "$pid" 2>/dev/null

echo "--- FashionCraft's output ---"
cat "$log"
echo "-----------------------------"

status=0
if [ "$alive" != yes ]; then
    echo "ERROR: FashionCraft exited after $waited s instead of staying up for $seconds s"
    status=1
fi
if [ -z "$window" ]; then
    echo "ERROR: no FashionCraft window appeared in $seconds s"
    status=1
fi
if grep -E 'could not be loaded|error while loading shared libraries|panicked at|^FashionCraft: ' "$log"; then
    echo "ERROR: FashionCraft's output above shows a missing library or a failed start"
    status=1
fi
if [ "$status" = 0 ]; then
    echo "OK: the window appeared after $window s and FashionCraft was still running after $seconds s"
fi
exit "$status"
