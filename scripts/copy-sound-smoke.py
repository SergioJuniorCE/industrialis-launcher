"""Observe real launcher copy bell requests on an owned Xvfb, without host audio/input."""
import contextlib
import ctypes as c
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import tempfile
import time


def load_helper(filename):
    spec = importlib.util.spec_from_file_location(filename.replace("-", "_"), Path(__file__).with_name(filename))
    helper = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(helper)
    return helper


class VirtualDesktop:
    def __init__(self, child):
        self.child = child
        self.windows = load_helper("virtual-game-window.py")
        self.x = c.CDLL("libX11.so.6")
        self.input = c.CDLL("libXtst.so.6")
        display, window = c.c_void_p, c.c_ulong
        signatures = {
            "XOpenDisplay": ([c.c_char_p], display),
            "XDefaultRootWindow": ([display], window),
            "XTranslateCoordinates": ([display, window, window, c.c_int, c.c_int,
                                       c.POINTER(c.c_int), c.POINTER(c.c_int), c.POINTER(window)], c.c_int),
            "XSetInputFocus": ([display, window, c.c_int, window], c.c_int),
            "XkbQueryExtension": ([display] + [c.POINTER(c.c_int)] * 5, c.c_int),
            "XkbSelectEvents": ([display, c.c_uint, c.c_uint, c.c_uint], c.c_int),
            "XPending": ([display], c.c_int),
            "XNextEvent": ([display, c.c_void_p], c.c_int),
            "XSync": ([display, c.c_int], c.c_int),
            "XCloseDisplay": ([display], c.c_int),
        }
        for name, (args, result) in signatures.items():
            function = getattr(self.x, name)
            function.argtypes, function.restype = args, result
        self.input.XTestFakeMotionEvent.argtypes = [display, c.c_int, c.c_int, c.c_int, window]
        self.input.XTestFakeButtonEvent.argtypes = [display, c.c_uint, c.c_int, window]
        self.display = self.x.XOpenDisplay(None)
        assert self.display, "No owned virtual display"
        opcode, event_base, error_base, major, minor = [c.c_int() for _ in range(5)]
        major.value, minor.value = 1, 0
        assert self.x.XkbQueryExtension(self.display, c.byref(opcode), c.byref(event_base),
                                        c.byref(error_base), c.byref(major), c.byref(minor)), "XKB unavailable"
        self.event_base = event_base.value
        assert self.x.XkbSelectEvents(self.display, 0x100, 1 << 8, 1 << 8), "Bell observation unavailable"
        self.x.XSync(self.display, 0)
        self.bells = []

    def owned_window(self):
        assert self.child.poll() is None, "App exited before acceptance completed"
        with contextlib.redirect_stdout(io.StringIO()):
            candidates = self.windows.close_game(self.child.pid, inspect_only=True)
        matches = [w for w in candidates if w[1] == [self.child.pid] and w[2] == 2]
        assert len(matches) == 1, "Expected one mapped window owned by the fixture app"
        return matches[0]

    def click(self, px, py):
        window, _, _, width, height = self.owned_window()
        assert 0 <= px < width and 0 <= py < height
        x, y, child = c.c_int(), c.c_int(), c.c_ulong()
        assert self.x.XTranslateCoordinates(self.display, window, self.x.XDefaultRootWindow(self.display),
                                           0, 0, c.byref(x), c.byref(y), c.byref(child))
        self.x.XSetInputFocus(self.display, window, 1, 0)
        self.input.XTestFakeMotionEvent(self.display, -1, x.value + px, y.value + py, 0)
        self.x.XSync(self.display, 0)
        time.sleep(0.05)
        self.input.XTestFakeButtonEvent(self.display, 1, 1, 0)
        self.x.XSync(self.display, 0)
        time.sleep(0.2)
        self.input.XTestFakeButtonEvent(self.display, 1, 0, 0)
        self.x.XSync(self.display, 0)
        time.sleep(0.3)

    def drain_bells(self):
        class Bell(c.Structure):
            _fields_ = [("type", c.c_int), ("serial", c.c_ulong), ("send_event", c.c_int),
                        ("display", c.c_void_p), ("time", c.c_ulong), ("xkb_type", c.c_int),
                        ("device", c.c_uint), ("percent", c.c_int), ("pitch", c.c_int),
                        ("duration", c.c_int), ("bell_class", c.c_uint), ("bell_id", c.c_uint),
                        ("name", c.c_ulong), ("window", c.c_ulong), ("event_only", c.c_int)]

        class Event(c.Union):
            _fields_ = [("bell", Bell), ("padding", c.c_long * 24)]

        self.x.XSync(self.display, 0)
        while self.x.XPending(self.display):
            event = Event()
            self.x.XNextEvent(self.display, c.byref(event))
            if event.bell.type == self.event_base and event.bell.xkb_type == 8:
                assert not event.bell.send_event and not event.bell.event_only, "Synthetic notification rejected"
                self.bells.append({"percent": event.bell.percent, "pitch": event.bell.pitch,
                                   "duration": event.bell.duration, "eventOnly": False})
        return len(self.bells)

    def wait(self, condition, description, seconds=15):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            self.owned_window()
            self.drain_bells()
            if condition():
                return
            time.sleep(0.05)
        raise AssertionError(description)

    def capture(self, output):
        window, _, _, width, height = self.owned_window()
        load_helper("linux-portable-smoke.py").capture(window, width, height, output)

    def pixel(self, px, py):
        window, _, _, width, height = self.owned_window()
        assert 0 <= px < width and 0 <= py < height
        self.x.XGetImage.argtypes = [c.c_void_p, c.c_ulong, c.c_int, c.c_int, c.c_uint, c.c_uint, c.c_ulong, c.c_int]
        self.x.XGetImage.restype = c.c_void_p
        self.x.XGetPixel.argtypes = [c.c_void_p, c.c_int, c.c_int]
        self.x.XGetPixel.restype = c.c_ulong
        self.x.XDestroyImage.argtypes = [c.c_void_p]
        image = self.x.XGetImage(self.display, window, px, py, 1, 1, c.c_ulong(-1), 2)
        assert image
        try:
            pixel = self.x.XGetPixel(image, 0, 0)
            return ((pixel >> 16) & 255, (pixel >> 8) & 255, pixel & 255)
        finally:
            self.x.XDestroyImage(image)


def run_case(binary, output, root, failure):
    profile = root / "profile"
    source = profile / "instances/source"
    (source / ".minecraft/config").mkdir(parents=True)
    (source / "mmc-pack.json").write_text("{}")
    (source / "instance.json").write_text(json.dumps({"name": "z-source", "pack_version": "fixture"}))
    payload = b"native copy bell acceptance\n"
    (source / ".minecraft/config/copy.cfg").write_bytes(payload)
    (profile / "launcher-settings.json").write_text(json.dumps({
        "window": {"window_width": 1100, "window_height": 750, "launch_maximized": False},
        "acceptanceExtension": True,
    }))
    env = dict(os.environ, HOME=str(root / "home"), XDG_CONFIG_HOME=str(root / "config"),
               INDUSTRIALIS_DATA_DIR=str(profile))
    label = "failure" if failure else "success"
    with (output / f"{label}.stdout.log").open("w") as stdout, (output / f"{label}.stderr.log").open("w") as stderr:
        child = subprocess.Popen([str(binary)], env=env, stdout=stdout, stderr=stderr)
        desktop = None
        try:
            desktop = VirtualDesktop(child)
            deadline = time.monotonic() + 20
            while True:
                try:
                    desktop.owned_window()
                    break
                except AssertionError:
                    assert child.poll() is None and time.monotonic() < deadline, "App did not map its window"
                    time.sleep(0.1)
            time.sleep(1)
            desktop.capture(output / f"{label}-before-copy.png")
            desktop.drain_bells()
            assert not desktop.bells, "Unexpected startup bell"
            desktop.click(75, 225)
            desktop.click(1020, 698)
            desktop.capture(output / f"{label}-copy-dialog.png")
            if failure:
                # Each case has one known tile. Retire that source only after
                # opening its Copy dialog; the worker must reject the stale source.
                source.rename(profile / "retired-source")
            desktop.click(700, 462)
            copies = lambda: [p for p in profile.joinpath("instances").iterdir() if p.name != "source"]
            if failure:
                time.sleep(3)
                assert not copies(), "Failed copy left a destination"
                assert desktop.drain_bells() == 0, "Failed copy requested a success bell"
                desktop.capture(output / "failed-copy.png")
                desktop.click(365, 20)
                desktop.capture(output / "failed-processes.png")
            else:
                desktop.wait(lambda: len(copies()) == 1 and (copies()[0] / ".minecraft/config/copy.cfg").is_file(),
                             "Native Copy action did not write the destination")
                desktop.wait(lambda: desktop.drain_bells() >= 1, "Successful copy did not request a native bell")
                assert (copies()[0] / ".minecraft/config/copy.cfg").read_bytes() == payload
                time.sleep(1)
                assert desktop.drain_bells() == 1, "Successful copy requested more than one bell"
                desktop.capture(output / "successful-copy.png")
            desktop.windows.close_game(child.pid)
            assert child.wait(timeout=15) == 0, "Normal app close failed"
            return desktop.bells
        finally:
            if desktop is not None:
                desktop.x.XCloseDisplay(desktop.display)
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=5)


def main():
    if (sys.platform != "linux" or not Path("/.dockerenv").is_file()
            or os.environ.get("LAUNCHER_ISOLATED_SOUND_CONTAINER") != "1"
            or os.environ.get("LAUNCHER_ISOLATED_VIRTUAL_DESKTOP") != "1"):
        raise RuntimeError("Only the explicitly owned disposable container/Xvfb is allowed")
    binary, output = [Path(arg).resolve() for arg in sys.argv[1:]]
    output.mkdir(parents=True, exist_ok=True)
    # A failed/repeated run must not retain a prior passing receipt.
    (output / "verification.json").unlink(missing_ok=True)
    with tempfile.TemporaryDirectory(prefix="industrialis-copy-sound-") as generated:
        root = Path(generated)
        successful_bells = run_case(binary, output, root / "success", False)
        failed_bells = run_case(binary, output, root / "failure", True)
    report = {"passed": True, "platform": f"Linux {platform.machine()} / owned Xvfb",
              "executableSha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
              "nativeSuccessfulCopyBells": len(successful_bells), "nativeFailedCopyBells": len(failed_bells),
              "copiedBytesVerified": True, "normalCloseExitCodes": [0, 0],
              "bellEvents": successful_bells, "physicalDesktopUsed": False,
              "limits": "Actual native bell requests, not physical speaker audibility or Windows/macOS/Wayland acceptance"}
    (output / "verification.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
