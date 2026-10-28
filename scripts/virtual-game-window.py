"""Send WM_DELETE_WINDOW only to the specified game PID on an isolated Xvfb display."""
import ctypes as c
import os
import sys


def close_game(pid, inspect_only=False):
    # Never use this helper against a physical user's desktop.
    if os.environ.get("LAUNCHER_ISOLATED_VIRTUAL_DESKTOP") != "1":
        raise RuntimeError("Requires the isolated acceptance desktop")
    x = c.CDLL("libX11.so.6")
    window = c.c_ulong
    display = c.c_void_p
    x.XOpenDisplay.argtypes = [c.c_char_p]
    x.XOpenDisplay.restype = display
    x.XDefaultRootWindow.argtypes = [display]
    x.XDefaultRootWindow.restype = window
    x.XInternAtom.argtypes = [display, c.c_char_p, c.c_int]
    x.XInternAtom.restype = window
    x.XQueryTree.argtypes = [display, window, c.POINTER(window), c.POINTER(window),
                             c.POINTER(c.POINTER(window)), c.POINTER(c.c_uint)]
    x.XGetWindowProperty.argtypes = [display, window, window, c.c_long, c.c_long,
                                     c.c_int, window, c.POINTER(window), c.POINTER(c.c_int),
                                     c.POINTER(c.c_ulong), c.POINTER(c.c_ulong),
                                     c.POINTER(c.POINTER(c.c_ubyte))]
    class Attributes(c.Structure):
        _fields_ = [("x", c.c_int), ("y", c.c_int), ("width", c.c_int), ("height", c.c_int),
                    ("border_width", c.c_int), ("depth", c.c_int), ("visual", c.c_void_p),
                    ("root", window), ("window_class", c.c_int), ("bit_gravity", c.c_int),
                    ("win_gravity", c.c_int), ("backing_store", c.c_int), ("backing_planes", window),
                    ("backing_pixel", window), ("save_under", c.c_int), ("colormap", window),
                    ("map_installed", c.c_int), ("map_state", c.c_int), ("all_event_masks", c.c_long),
                    ("your_event_mask", c.c_long), ("do_not_propagate_mask", c.c_long),
                    ("override_redirect", c.c_int), ("screen", c.c_void_p)]
    x.XGetWindowAttributes.argtypes = [display, window, c.POINTER(Attributes)]
    x.XFree.argtypes = [c.c_void_p]
    x.XCloseDisplay.argtypes = [display]
    d = x.XOpenDisplay(None)
    if not d:
        raise RuntimeError("No virtual display")
    try:
        # LWJGL 2 lacks _NET_WM_PID. XRes 1.2 reports the owning local X client's
        # PID from the server, so an absent property never requires title guessing.
        try:
            res = c.CDLL("libXRes.so.1")
        except OSError:
            res = None

        class ClientSpec(c.Structure):
            _fields_ = [("client", window), ("mask", c.c_uint)]

        class ClientValue(c.Structure):
            _fields_ = [("spec", ClientSpec), ("length", c.c_long), ("value", c.c_void_p)]

        if res:
            res.XResQueryVersion.argtypes = [display, c.POINTER(c.c_int), c.POINTER(c.c_int)]
            major, minor = c.c_int(), c.c_int()
            res.XResQueryVersion(d, c.byref(major), c.byref(minor))
            if (major.value, minor.value) < (1, 2):
                res = None
        if res:
            res.XResQueryClientIds.argtypes = [display, c.c_long, c.POINTER(ClientSpec),
                                               c.POINTER(c.c_long), c.POINTER(c.POINTER(ClientValue))]
            res.XResGetClientIdType.argtypes = [c.POINTER(ClientValue)]
            res.XResGetClientPid.argtypes = [c.POINTER(ClientValue)]
            res.XResGetClientPid.restype = c.c_int
            res.XResClientIdsDestroy.argtypes = [c.c_long, c.POINTER(ClientValue)]

        def server_owners(w):
            if not res:
                return []
            spec, count, values = ClientSpec(w, 2), c.c_long(), c.POINTER(ClientValue)()
            res.XResQueryClientIds(d, 1, c.byref(spec), c.byref(count), c.byref(values))
            try:
                if not values or count.value != 1 or res.XResGetClientIdType(values) != 1:
                    return []
                owner = res.XResGetClientPid(values)
                return [owner] if owner > 1 else []
            finally:
                if values:
                    res.XResClientIdsDestroy(count.value, values)

        def prop(w, name):
            actual, fmt, count, remaining = window(), c.c_int(), c.c_ulong(), c.c_ulong()
            data = c.POINTER(c.c_ubyte)()
            status = x.XGetWindowProperty(d, w, x.XInternAtom(d, name, 0), 0, 1024, 0, 0,
                                         c.byref(actual), c.byref(fmt), c.byref(count),
                                         c.byref(remaining), c.byref(data))
            try:
                if status or fmt.value != 32 or not data:
                    return []
                return list(c.cast(data, c.POINTER(c.c_ulong))[:count.value])
            finally:
                if data:
                    x.XFree(data)

        def descendants(w):
            root, parent, count = window(), window(), c.c_uint()
            children = c.POINTER(window)()
            if x.XQueryTree(d, w, c.byref(root), c.byref(parent), c.byref(children), c.byref(count)):
                try:
                    found = list(children[:count.value])
                finally:
                    if children:
                        x.XFree(children)
                for child in found:
                    yield child
                    yield from descendants(child)

        delete = x.XInternAtom(d, b"WM_DELETE_WINDOW", 0)
        candidates = []
        for w in descendants(x.XDefaultRootWindow(d)):
            if delete in prop(w, b"WM_PROTOCOLS"):
                attrs = Attributes()
                if x.XGetWindowAttributes(d, w, c.byref(attrs)):
                    owners = prop(w, b"_NET_WM_PID") or server_owners(w)
                    candidates.append((w, owners, attrs.map_state,
                                       attrs.width, attrs.height))
        if inspect_only:
            print(f"Virtual closeable window owner PIDs: {candidates}; target PID: {pid}")
            return candidates
        # SDL also owns hidden 1x1 rendering windows with the same PID/protocols.
        # X11 IsViewable (2) identifies the actual visible game window.
        matches = [w for w, owners, state, _, _ in candidates if owners == [pid] and state == 2]
        if len(matches) != 1:
            raise RuntimeError(f"Expected exactly one closeable window owned by game PID {pid}; candidates: {candidates}")

        class Data(c.Union):
            _fields_ = [("b", c.c_char * 20), ("s", c.c_short * 10), ("l", c.c_long * 5)]

        class Message(c.Structure):
            _fields_ = [("type", c.c_int), ("serial", c.c_ulong), ("send_event", c.c_int),
                        ("display", display), ("window", window), ("message_type", window),
                        ("format", c.c_int), ("data", Data)]

        class Event(c.Union):
            _fields_ = [("message", Message), ("padding", c.c_long * 24)]

        event = Event()
        event.message = Message(33, 0, 1, d, matches[0],
                                x.XInternAtom(d, b"WM_PROTOCOLS", 0), 32, Data())
        event.message.data.l[0] = delete
        x.XSendEvent.argtypes = [display, window, c.c_int, c.c_long, c.POINTER(Event)]
        x.XFlush.argtypes = [display]
        if not x.XSendEvent(d, matches[0], 0, 0, c.byref(event)):
            raise RuntimeError("Window rejected close event")
        x.XFlush(d)
    finally:
        x.XCloseDisplay(d)


if __name__ == "__main__":
    if len(sys.argv) not in (2, 3) or int(sys.argv[1]) <= 1 or (len(sys.argv) == 3 and sys.argv[2] != "--inspect"):
        raise RuntimeError("Usage: virtual-game-window.py <owned-game-pid> [--inspect]")
    close_game(int(sys.argv[1]), len(sys.argv) == 3)
