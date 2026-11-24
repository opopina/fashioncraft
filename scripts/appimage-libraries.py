#!/usr/bin/env python3
"""Copies into the AppImage the shared libraries FashionCraft needs that a Linux system may lack.

    appimage-libraries.py LIBDIR PROGRAM... -- SONAME...

PROGRAM...  the AppDir's executables: their NEEDED libraries are bundled, and their RUNPATH set
            to $ORIGIN/../lib (LIBDIR beside bin/).
SONAME...   libraries the program opens at run time with dlopen, which no tool sees in the
            executable: winit opens xkbcommon, Xcursor and Xi, wgpu opens wayland-egl. They are
            bundled too, and a missing one stops the build.

Each library brings what it needs in turn, except what the AppImage project's excludelist says
must come from the system (glibc, the graphics drivers' libGL/libEGL, libX11, libxcb, ...). The
libraries get RUNPATH $ORIGIN, so each finds the next one in LIBDIR. RUNPATH, not
LD_LIBRARY_PATH in AppRun: the programs FashionCraft starts (Python, ffmpeg, the AI assistants'
command-line tools, the file dialog's zenity) must keep the system's own libraries.

The libraries' copyright files go to usr/share/doc/fashioncraft/libraries/ (their licences, MIT
for X11, xkbcommon and Wayland, ask for the notice to travel with the copies).
"""
import os
import shutil
import subprocess
import sys

# https://github.com/AppImageCommunity/pkg2appimage/blob/master/excludelist (November 2026):
# libraries every system has, or must provide itself because they go with its kernel, C library
# or graphics driver. libvulkan.so.1 is not on it, but the Vulkan loader has to match the system's
# drivers (its ICDs); without it wgpu uses OpenGL through EGL.
EXCLUDED = set("""
ld-linux.so.2 ld-linux-x86-64.so.2 ld-linux-aarch64.so.1 libanl.so.1 libBrokenLocale.so.1
libcidn.so.1 libc.so.6 libdl.so.2 libm.so.6 libmvec.so.1 libnss_compat.so.2 libnss_dns.so.2
libnss_files.so.2 libnss_hesiod.so.2 libnss_nisplus.so.2 libnss_nis.so.2 libpthread.so.0
libresolv.so.2 librt.so.1 libthread_db.so.1 libutil.so.1 libstdc++.so.6 libGL.so.1 libEGL.so.1
libGLdispatch.so.0 libGLX.so.0 libOpenGL.so.0 libdrm.so.2 libglapi.so.0 libgbm.so.1 libxcb.so.1
libX11.so.6 libX11-xcb.so.1 libwayland-client.so.0 libasound.so.2 libfontconfig.so.1
libfreetype.so.6 libharfbuzz.so.0 libcom_err.so.2 libexpat.so.1 libgcc_s.so.1 libgpg-error.so.0
libICE.so.6 libSM.so.6 libusb-1.0.so.0 libuuid.so.1 libz.so.1 libjack.so.0 libpipewire-0.3.so.0
libxcb-dri3.so.0 libxcb-dri2.so.0 libfribidi.so.0 libgmp.so.10
libvulkan.so.1
""".split())


def needed(path):
    out = subprocess.run(["readelf", "-d", path], check=True, capture_output=True, text=True).stdout
    return [line.split("[", 1)[1].split("]", 1)[0] for line in out.splitlines() if "(NEEDED)" in line]


def system_libraries():
    """soname -> path, for this machine's architecture, from the dynamic linker's cache."""
    arch = os.uname().machine.replace("_", "-")  # x86-64, aarch64
    found = {}
    for line in subprocess.run(["ldconfig", "-p"], check=True, capture_output=True, text=True).stdout.splitlines()[1:]:
        name, _, rest = line.strip().partition(" (")
        flags, _, path = rest.partition(") => ")
        if arch in flags or (arch == "aarch64" and "AArch64" in flags):
            found.setdefault(name, path)
    return found


def copyright_file(path):
    """The Debian package's copyright file of a library, if dpkg knows the file."""
    real = os.path.realpath(path)
    for candidate in (real, real.replace("/usr/lib/", "/lib/", 1)):
        result = subprocess.run(["dpkg", "-S", candidate], capture_output=True, text=True)
        if result.returncode == 0:
            package = result.stdout.split(":", 1)[0]
            doc = f"/usr/share/doc/{package}/copyright"
            return package, doc if os.path.exists(doc) else None
    return None, None


def main(argv):
    if "--" not in argv:
        sys.exit(__doc__)
    split = argv.index("--")
    libdir, programs, opened = argv[0], argv[1:split], argv[split + 1:]
    os.makedirs(libdir, exist_ok=True)
    docs = os.path.join(os.path.dirname(libdir), "share", "doc", "fashioncraft", "libraries")
    system = system_libraries()

    missing = [name for name in opened if name not in system]
    if missing:
        sys.exit(f"appimage-libraries: not installed on this machine: {' '.join(missing)}")

    queue = [name for program in programs for name in needed(program)] + opened
    bundled = []
    while queue:
        name = queue.pop(0)
        if name in EXCLUDED or name in bundled:
            continue
        if name not in system:
            sys.exit(f"appimage-libraries: {name} is needed but not installed on this machine")
        target = os.path.join(libdir, name)
        shutil.copyfile(system[name], target)
        os.chmod(target, 0o755)
        subprocess.run(["patchelf", "--set-rpath", "$ORIGIN", target], check=True)
        bundled.append(name)
        package, doc = copyright_file(system[name])
        if doc:
            os.makedirs(docs, exist_ok=True)
            shutil.copyfile(doc, os.path.join(docs, f"{package}.copyright"))
        print(f"bundled {name} ({package or 'unknown package'})")
        queue.extend(needed(target))

    for program in programs:
        subprocess.run(["patchelf", "--set-rpath", "$ORIGIN/../lib", program], check=True)


if __name__ == "__main__":
    main(sys.argv[1:])
