"""Native file editor checks on a fresh private Xvfb, never the user's desktop."""
import ctypes as c
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
from importlib.util import spec_from_file_location, module_from_spec


def keys(desktop, text=None, find=False, escape=False):
    window = desktop.owned_window()[0]
    desktop.x.XSetInputFocus(desktop.display, window, 1, 0)
    desktop.x.XKeysymToKeycode.argtypes = [c.c_void_p, c.c_ulong]
    desktop.x.XKeysymToKeycode.restype = c.c_uint
    desktop.input.XTestFakeKeyEvent.argtypes = [c.c_void_p, c.c_uint, c.c_int, c.c_ulong]
    def key(symbol, pressed):
        code = desktop.x.XKeysymToKeycode(desktop.display, symbol)
        assert code
        desktop.input.XTestFakeKeyEvent(desktop.display, code, pressed, 0)
    if find:
        key(0xffe3, 1)
        key(ord('f'), 1)
        key(ord('f'), 0)
        key(0xffe3, 0)
    if escape:
        key(0xff1b, 1)
        key(0xff1b, 0)
    if text:
        assert text.isascii() and text.islower()
        for char in text:
            key(ord(char), 1)
            key(ord(char), 0)
    desktop.x.XSync(desktop.display, 0)
    time.sleep(0.3)


def code_ready(desktop):
    # This generated fixture starts with a comment. Wait for its actual painted
    # glyphs, not just the selected mode while the file request is still pending.
    selected = desktop.pixel(1254, 246)
    glyph = desktop.pixel(1172, 312)
    background = desktop.pixel(1480, 312)
    return selected[0] > selected[2] + 25 and sum(abs(a - b) for a, b in zip(glyph, background)) > 60


def main():
    if (sys.platform != 'linux' or not Path('/.dockerenv').is_file()
            or os.environ.get('LAUNCHER_ISOLATED_EDITOR_CONTAINER') != '1'
            or os.environ.get('LAUNCHER_ISOLATED_VIRTUAL_DESKTOP') != '1'):
        raise RuntimeError('Requires the owned disposable container/virtual desktop')
    binary, output = [Path(arg).resolve() for arg in sys.argv[1:3]]
    probe = len(sys.argv) > 3 and sys.argv[3] in ['probe', 'code-probe']
    code_probe = len(sys.argv) > 3 and sys.argv[3] == 'code-probe'
    output.mkdir(parents=True, exist_ok=True)
    if not probe:
        (output / 'verification.json').unlink(missing_ok=True)
    spec = spec_from_file_location('copy_desktop', Path(__file__).with_name('copy-sound-smoke.py'))
    helper = module_from_spec(spec)
    spec.loader.exec_module(helper)
    receipts = []
    for mode in (['dark'] if probe else ['dark', 'light']):
        with tempfile.TemporaryDirectory(prefix='industrialis-editor-native-') as fresh:
            root = Path(fresh)
            profile = root / 'profile'
            instance = profile / 'instances/editor'
            minecraft = instance / '.minecraft'
            minecraft.mkdir(parents=True)
            (instance / 'mmc-pack.json').write_text('{}')
            (instance / 'instance.json').write_text(json.dumps({'name': 'Editor acceptance', 'pack_version': 'fixture'}))
            source = '# Keep this comment\ngeneral {\n    B:enabled=true\n    I:count=42\n    S:message="Unicode 中"\n    S:labels <\n        first\n        second\n    >\n}\n'
            file = minecraft / 'config-editor.cfg'
            file.write_text(source)
            (profile / 'launcher-settings.json').write_text(json.dumps({
                'theme_mode': mode, 'theme_preset': 'sandstone', 'acceptanceExtension': True,
                'window_width': 1600, 'window_height': 900, 'launch_maximized': False}))
            env = dict(os.environ, HOME=str(root / 'home'), XDG_CONFIG_HOME=str(root / 'config'),
                       INDUSTRIALIS_DATA_DIR=str(profile))
            with (output / f'{mode}.stdout.log').open('w') as stdout, (output / f'{mode}.stderr.log').open('w') as stderr:
                child = subprocess.Popen([str(binary)], env=env, stdout=stdout, stderr=stderr)
                desktop = None
                try:
                    desktop = helper.VirtualDesktop(child)
                    deadline = time.monotonic() + 60
                    while True:
                        try:
                            desktop.owned_window()
                            break
                        except AssertionError:
                            assert child.poll() is None and time.monotonic() < deadline, f'Native window startup failed: exit={child.poll()}'
                            time.sleep(0.1)
                    time.sleep(1)
                    assert desktop.owned_window()[3:5] == (1600, 900), 'Fixture viewport does not match its settings'
                    desktop.click(75, 225)
                    desktop.click(1280, 118)
                    time.sleep(0.8)
                    desktop.capture(output / f'{mode}-browser.png')
                    if code_probe:
                        desktop.click(940, 297)
                        desktop.wait(lambda: code_ready(desktop),
                                     'Advanced mode did not render after file activation')
                        desktop.capture(output / f'{mode}-advanced.png')
                    if not probe:
                        # Coordinates are scoped to this fixed-size owned fixture;
                        # probe screenshots establish the actual native layout.
                        desktop.click(940, 297)
                        desktop.wait(lambda: code_ready(desktop),
                                     'Advanced mode did not render after file activation')
                        desktop.capture(output / f'{mode}-advanced.png')
                        desktop.click(1310, 480)
                        keys(desktop, find=True)
                        keys(desktop, text='count')
                        desktop.capture(output / f'{mode}-find.png')
                        keys(desktop, escape=True)
                        desktop.click(1132, 256)
                        desktop.capture(output / f'{mode}-easy.png')
                        desktop.click(1230, 308)
                        desktop.capture(output / f'{mode}-easy-edited.png')
                        desktop.click(1135, 670)
                        desktop.wait(lambda: 'B:enabled=false' in file.read_text(), 'Native Easy edit did not save')
                        overlay = instance / 'persistent-minecraft/config-editor.cfg'
                        assert file.read_text() == source.replace('B:enabled=true', 'B:enabled=false')
                        assert overlay.read_bytes() == file.read_bytes()
                        assert json.loads((profile / 'launcher-settings.json').read_text())['acceptanceExtension']
                        receipts.append({'mode': mode, 'easySavedBothCopies': True,
                                         'findDidNotAlterDocument': True,
                                         'unknownPreferencesPreserved': True})
                    desktop.windows.close_game(child.pid)
                    assert child.wait(timeout=15) == 0
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
    if not probe:
        receipt = {'passed': True, 'executableSha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                   'platform': 'Linux x64 / owned private Xvfb', 'modes': receipts, 'normalCloseExitCodes': [0, 0],
                   'physicalDesktopUsed': False, 'limits': 'Generated profiles; no physical IME or original-account tests'}
        (output / 'verification.json').write_text(json.dumps(receipt, indent=2) + '\n')
        print(json.dumps(receipt, indent=2))


if __name__ == '__main__':
    main()
