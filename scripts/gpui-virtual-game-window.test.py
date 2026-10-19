"""Opt-in Xvfb regression for the actual client's hidden SDL window pattern."""
import ctypes as c
import importlib.util
import os
from pathlib import Path

assert os.environ.get("GPUI_ISOLATED_VIRTUAL_DESKTOP") == "1", "Run on an isolated Xvfb display"
spec = importlib.util.spec_from_file_location("window_helper", Path(__file__).with_name("gpui-virtual-game-window.py"))
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)
del os.environ["GPUI_ISOLATED_VIRTUAL_DESKTOP"]
try:
    helper.close_game(os.getpid())
    raise AssertionError("Missing isolated-desktop guard")
except RuntimeError as error:
    assert "isolated" in str(error)
os.environ["GPUI_ISOLATED_VIRTUAL_DESKTOP"] = "1"

x = c.CDLL("libX11.so.6")
D, W = c.c_void_p, c.c_ulong
x.XOpenDisplay.argtypes = [c.c_char_p]
x.XOpenDisplay.restype = D
x.XDefaultRootWindow.argtypes = [D]
x.XDefaultRootWindow.restype = W
x.XCreateSimpleWindow.argtypes = [D, W, c.c_int, c.c_int, c.c_uint, c.c_uint, c.c_uint, W, W]
x.XCreateSimpleWindow.restype = W
x.XInternAtom.argtypes = [D, c.c_char_p, c.c_int]
x.XInternAtom.restype = W
x.XChangeProperty.argtypes = [D, W, W, W, c.c_int, c.c_int, c.c_void_p, c.c_int]
x.XDeleteProperty.argtypes = [D, W, W]
x.XSetWMProtocols.argtypes = [D, W, c.POINTER(W), c.c_int]
x.XMapWindow.argtypes = [D, W]
x.XUnmapWindow.argtypes = [D, W]
x.XSync.argtypes = [D, c.c_int]
x.XCloseDisplay.argtypes = [D]
d = x.XOpenDisplay(None)
assert d
try:
    root = x.XDefaultRootWindow(d)
    pid = (W * 1)(os.getpid())
    protocols = (W * 1)(x.XInternAtom(d, b"WM_DELETE_WINDOW", 0))

    def create(width, height):
        w = x.XCreateSimpleWindow(d, root, 0, 0, width, height, 0, 0, 0)
        x.XChangeProperty(d, w, x.XInternAtom(d, b"_NET_WM_PID", 0), 6, 32, 0, pid, 1)
        x.XSetWMProtocols(d, w, protocols, 1)
        return w

    visible, auxiliary = create(100, 100), create(1, 1)
    x.XMapWindow(d, visible)
    x.XMapWindow(d, auxiliary)
    x.XSync(d, 0)
    try:
        helper.close_game(os.getpid())
        raise AssertionError("Multiple visible owned windows must be rejected")
    except RuntimeError as error:
        assert "exactly one" in str(error)
    x.XUnmapWindow(d, auxiliary)
    x.XSync(d, 0)
    inspected = helper.close_game(os.getpid(), inspect_only=True)
    assert {w[0] for w in inspected} == {visible, auxiliary}
    assert next(w for w in inspected if w[0] == visible)[2:] == (2, 100, 100)
    helper.close_game(os.getpid())
    event = (c.c_long * 24)()
    x.XNextEvent.argtypes = [D, c.c_void_p]
    x.XNextEvent(d, c.byref(event))
    assert c.cast(c.byref(event), c.POINTER(c.c_int))[0] == 33
    # XClientMessageEvent's window field follows its display pointer.
    assert event[4] == visible
    # LWJGL 2 does not publish _NET_WM_PID. The X server must establish ownership.
    x.XDeleteProperty(d, visible, x.XInternAtom(d, b"_NET_WM_PID", 0))
    x.XSync(d, 0)
    try:
        helper.close_game(os.getpid() + 100000)
        raise AssertionError("An unrelated PID must not own the property-free window")
    except RuntimeError as error:
        assert "exactly one" in str(error)
    helper.close_game(os.getpid())
    x.XNextEvent(d, c.byref(event))
    assert c.cast(c.byref(event), c.POINTER(c.c_int))[0] == 33
    assert event[4] == visible
    print("Isolated guard, ambiguous/hidden windows and property-free X-server ownership passed")
finally:
    x.XCloseDisplay(d)
