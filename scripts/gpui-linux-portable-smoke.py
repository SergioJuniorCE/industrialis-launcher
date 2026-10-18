"""Verify and boot a locally built Linux portable archive on an isolated Xvfb display."""
import contextlib
import ctypes as c
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import platform
import shutil
import struct
import subprocess
import sys
import tarfile
import tempfile
import time
import zlib

FILES = {"industrialis", "LICENSE", "THIRD_PARTY_NOTICES.txt", "README.txt", "icon.png", "industrialis-gpui.desktop"}


def unpack_archive(archive, root):
    # Validate every member before writing anything. Copy only the six known
    # regular files, without tar's link/path handling or newer Python filters.
    with tarfile.open(archive, "r:gz") as tar:
        members = tar.getmembers()
        files = [member for member in members if member.isfile()]
        assert len(files) == len(FILES) and {m.name for m in files} == {f"industrialis-gpui/{name}" for name in FILES}, "Unexpected or duplicate portable files"
        assert all(m.isfile() or (m.isdir() and m.name.rstrip("/") == "industrialis-gpui") for m in members), "Portable contains links or unexpected paths"
        folder = root / "industrialis-gpui"
        folder.mkdir(parents=True, exist_ok=False)
        for member in files:
            destination = folder / Path(member.name).name
            with tar.extractfile(member) as source, destination.open("xb") as output:
                shutil.copyfileobj(source, output)
            destination.chmod(member.mode & 0o777)
        return folder


def sha(data):
    return hashlib.sha256(data).hexdigest()


def verify_second_launch_activation(helper, child, binary, folder, env, owned_window, command=None):
    if sys.platform != "linux" or os.environ.get("GPUI_ISOLATED_VIRTUAL_DESKTOP") != "1":
        raise RuntimeError("Requires the owned virtual desktop")
    with contextlib.redirect_stdout(io.StringIO()):
        windows = helper.close_game(child.pid, inspect_only=True)
    assert any(w[0] == owned_window and w[1] == [child.pid] and w[2] == 2 for w in windows), "Window identity changed"
    x = c.CDLL("libX11.so.6")
    display_type, window_type = c.c_void_p, c.c_ulong
    x.XOpenDisplay.argtypes = [c.c_char_p]
    x.XOpenDisplay.restype = display_type
    x.XDefaultRootWindow.argtypes = [display_type]
    x.XDefaultRootWindow.restype = window_type
    x.XDefaultScreen.argtypes = [display_type]
    x.XIconifyWindow.argtypes = [display_type, window_type, c.c_int]
    x.XInternAtom.argtypes = [display_type, c.c_char_p, c.c_int]
    x.XInternAtom.restype = window_type
    x.XGetWindowProperty.argtypes = [display_type, window_type, window_type, c.c_long, c.c_long,
                                   c.c_int, window_type, c.POINTER(window_type), c.POINTER(c.c_int),
                                   c.POINTER(c.c_ulong), c.POINTER(c.c_ulong), c.POINTER(c.POINTER(c.c_ubyte))]
    x.XSync.argtypes = [display_type, c.c_int]
    x.XFree.argtypes = [c.c_void_p]
    x.XCloseDisplay.argtypes = [display_type]
    display = x.XOpenDisplay(None)
    assert display, "Missing owned virtual display"
    try:
        def state():
            assert child.poll() is None, "Owner exited during activation"
            with contextlib.redirect_stdout(io.StringIO()):
                candidates = helper.close_game(child.pid, inspect_only=True)
            found = [w for w in candidates if w[0] == owned_window and w[1] == [child.pid]]
            assert len(found) == 1, "Owned window identity lost"
            return found[0][2]

        def foreground():
            actual, fmt, count, remaining = window_type(), c.c_int(), c.c_ulong(), c.c_ulong()
            data = c.POINTER(c.c_ubyte)()
            status = x.XGetWindowProperty(display, x.XDefaultRootWindow(display),
                                         x.XInternAtom(display, b"_NET_ACTIVE_WINDOW", 0), 0, 1, 0, 0,
                                         c.byref(actual), c.byref(fmt), c.byref(count), c.byref(remaining), c.byref(data))
            try:
                return not status and fmt.value == 32 and count.value == 1 and bool(data) and c.cast(data, c.POINTER(window_type))[0] == owned_window
            finally:
                if data:
                    x.XFree(data)

        assert x.XIconifyWindow(display, owned_window, x.XDefaultScreen(display)), "Owned minimize request failed"
        x.XSync(display, 0)
        deadline = time.monotonic() + 15
        while state() == 2:
            assert time.monotonic() < deadline, "Owned window did not minimize"
            time.sleep(0.1)
        second = subprocess.run(command or [str(binary)], cwd=folder, env=env, capture_output=True, text=True, timeout=15)
        assert second.returncode == 0, "Second launcher did not hand off successfully"
        deadline = time.monotonic() + 15
        while state() != 2 or not foreground():
            assert time.monotonic() < deadline, "Second launch did not restore and foreground its owner"
            time.sleep(0.1)
        return {"secondLaunchExitCode": 0, "sameOwnerRestoredFromMinimize": True, "sameOwnerForeground": True}
    finally:
        x.XCloseDisplay(display)


def capture(window, width, height, destination):
    x = c.CDLL("libX11.so.6")
    x.XOpenDisplay.argtypes = [c.c_char_p]
    x.XOpenDisplay.restype = c.c_void_p
    x.XGetImage.argtypes = [c.c_void_p, c.c_ulong, c.c_int, c.c_int, c.c_uint, c.c_uint, c.c_ulong, c.c_int]
    x.XGetImage.restype = c.c_void_p
    x.XGetPixel.argtypes = [c.c_void_p, c.c_int, c.c_int]
    x.XGetPixel.restype = c.c_ulong
    x.XDestroyImage.argtypes = [c.c_void_p]
    x.XCloseDisplay.argtypes = [c.c_void_p]
    display = x.XOpenDisplay(None)
    assert display, "Missing virtual display"
    image = x.XGetImage(display, window, 0, 0, width, height, c.c_ulong(-1), 2)
    try:
        assert image, "Could not read the owned window"
        colors = set()
        pixels = bytearray()
        for py in range(height):
            pixels.append(0)
            for px in range(width):
                pixel = x.XGetPixel(image, px, py) & 0xffffff
                colors.add(pixel)
                pixels.extend(((pixel >> 16) & 255, (pixel >> 8) & 255, pixel & 255))
        def chunk(kind, data):
            return struct.pack("!I", len(data)) + kind + data + struct.pack("!I", zlib.crc32(kind + data))
        destination.write_bytes(b"\x89PNG\r\n\x1a\n" +
                                chunk(b"IHDR", struct.pack("!IIBBBBB", width, height, 8, 2, 0, 0, 0)) +
                                chunk(b"IDAT", zlib.compress(pixels)) + chunk(b"IEND", b""))
        return len(colors)
    finally:
        if image:
            x.XDestroyImage(image)
        x.XCloseDisplay(display)


def wait_for_render(child, window, width, height, destination, timeout=60):
    # Mapping the window does not mean its first GPU frame has been presented.
    # Keep the same nonblank-frame requirement, but inspect all pixels and keep
    # the last PNG even on failure so CI can distinguish delay from broken paint.
    deadline = time.monotonic() + timeout
    while True:
        assert child.poll() is None, "Portable launcher exited before rendering"
        colors = capture(window, width, height, destination)
        if colors >= 8:
            return colors
        assert time.monotonic() < deadline, (
            f"Portable window has not rendered its UI: {colors} distinct colors; see {destination}"
        )
        time.sleep(0.2)


def main():
    assert sys.platform == "linux" and os.environ.get("GPUI_ISOLATED_VIRTUAL_DESKTOP") == "1"
    assert len(sys.argv) == 4, "Usage: gpui-linux-portable-smoke.py <archive> <compiled-binary> <output-dir>"
    archive, reference, output = (Path(arg).resolve(strict=True) for arg in sys.argv[1:])
    assert output == archive.parent, "Keep evidence next to the generated artifact"
    channel = os.environ.get("GPUI_PACKAGE_CHANNEL", "preview")
    assert channel in ("preview", "stable"), "Unknown package channel"
    repo = Path(__file__).resolve().parent.parent
    app = repo / "apps/launcher-gpui"
    version = json.loads((app / "package.json").read_text())["version"]
    spec = importlib.util.spec_from_file_location("virtual_window", repo / "scripts/gpui-virtual-game-window.py")
    helper = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(helper)
    with tempfile.TemporaryDirectory(prefix="gpui-portable-smoke-") as temporary:
        root = Path(temporary)
        folder = unpack_archive(archive, root)
        binary = folder / "industrialis"
        contents = binary.read_bytes()
        assert contents == reference.read_bytes(), "Portable executable differs from its compiled input"
        assert contents[:6] == b"\x7fELF\x02\x01" and int.from_bytes(contents[18:20], "little") == 62
        assert binary.stat().st_mode & 0o111 == 0o111
        assert (folder / "LICENSE").read_bytes() == (app / "LICENSE").read_bytes()
        assert (folder / "icon.png").read_bytes() == (app / "crates/app/assets/brand.png").read_bytes()
        notices = (folder / "THIRD_PARTY_NOTICES.txt").read_text()
        assert "gpui-pre" in notices and (app / "crates/app/assets/LICENSE-LUCIDE").read_text() in notices
        assert version in (folder / "README.txt").read_text() and "manual" in (folder / "README.txt").read_text()
        registers_protocol = "MimeType=x-scheme-handler/industrialislauncher;" in (folder / "industrialis-gpui.desktop").read_text()
        assert registers_protocol == (channel == "stable" or os.environ.get("GPUI_PACKAGE_REPLACEMENT") == "1"), "Desktop protocol registration differs from package options"
        dependencies = subprocess.run(["ldd", str(binary)], capture_output=True, text=True, check=True)
        assert "not found" not in dependencies.stdout + dependencies.stderr
        # Verify the actual normal Linux data path, without a profile override,
        # while keeping HOME/XDG and desktop registration inside this fixture.
        home = root / "home"
        config, data, cache = home / ".config", home / ".local/share", home / ".cache"
        profile = config / "industrialis-launcher"
        profile.mkdir(parents=True)
        settings = profile / "launcher-settings.json"
        settings.write_text(json.dumps({"acceptanceExtension": {"preserve": True}}))
        env = {**os.environ, "HOME": str(home), "XDG_CONFIG_HOME": str(config),
               "XDG_DATA_HOME": str(data), "XDG_CACHE_HOME": str(cache)}
        env.pop("INDUSTRIALIS_DATA_DIR", None)
        if registers_protocol:
            applications = data / "applications"
            applications.mkdir(parents=True)
            shutil.copyfile(folder / "industrialis-gpui.desktop", applications / "industrialis-gpui.desktop")
            env["PATH"] = str(folder) + os.pathsep + env["PATH"]
            env["XDG_CURRENT_DESKTOP"] = "X-Generic"
            subprocess.run(["xdg-mime", "default", "industrialis-gpui.desktop", "x-scheme-handler/industrialislauncher"], env=env, check=True, timeout=15)
            registered = subprocess.run(["xdg-mime", "query", "default", "x-scheme-handler/industrialislauncher"], env=env, capture_output=True, text=True, check=True, timeout=15)
            assert registered.stdout.strip() == "industrialis-gpui.desktop", "Owned desktop scheme registration failed"
        with (output / "portable-native.log").open("w") as log:
            child = subprocess.Popen([str(binary)], cwd=folder, env=env, stdout=log, stderr=log)
            try:
                deadline = time.monotonic() + 60
                matches = []
                while time.monotonic() < deadline:
                    assert child.poll() is None, "Portable launcher exited before opening its window"
                    with contextlib.redirect_stdout(io.StringIO()):
                        windows = helper.close_game(child.pid, inspect_only=True)
                    matches = [w for w in windows if w[1] == [child.pid] and w[2] == 2]
                    if len(matches) == 1 and matches[0][3] >= 800 and matches[0][4] >= 600:
                        break
                    time.sleep(0.2)
                assert len(matches) == 1, "Portable launcher has no unique visible owned window"
                window, _, _, width, height = matches[0]
                colors = wait_for_render(child, window, width, height, output / "portable-render.png")
                activation = verify_second_launch_activation(helper, child, binary, folder, env, window)
                protocol = (verify_second_launch_activation(helper, child, binary, folder, env, window,
                            ["xdg-open", "industrialislauncher://owned-acceptance"])
                            if registers_protocol else None)
                helper.close_game(child.pid)
                assert child.wait(timeout=60) == 0, "Portable launcher did not close normally"
            finally:
                if child.poll() is None:
                    child.terminate()
                    try:
                        child.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        child.kill()
                        child.wait()
        assert json.loads(settings.read_text())["acceptanceExtension"] == {"preserve": True}
        os_release = platform.freedesktop_os_release()
        report = {"platform": f"{os_release.get('PRETTY_NAME', 'Linux')} {platform.machine()} / isolated Xvfb", "version": version, "channel": channel,
                  "artifactSha256": sha(archive.read_bytes()), "executableSha256": sha(contents),
                  "files": sorted(FILES), "noticesAndLicense": True, "dependenciesPresent": True,
                  "nativeWindow": {"width": width, "height": height, "sampledColors": colors},
                  "normalCloseExitCode": 0, "unknownSettingsPreserved": True, "updates": "manual",
                  "activation": activation,
                  "defaultLinuxProfilePath": True, "registeredProtocolActivation": protocol,
                  "limits": "Generated profile only; no physical input, Wayland, original credentials or production distribution acceptance"}
        (output / "verification.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
