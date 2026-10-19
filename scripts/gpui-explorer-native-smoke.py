"""Native Explorer checks on an explicitly owned disposable Docker/Xvfb."""
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


def load(filename):
    spec = spec_from_file_location(filename, Path(__file__).with_name(filename))
    module = module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def tree_keys(desktop, symbols):
    desktop.x.XKeysymToKeycode.argtypes = [c.c_void_p, c.c_ulong]
    desktop.x.XKeysymToKeycode.restype = c.c_uint
    desktop.input.XTestFakeKeyEvent.argtypes = [c.c_void_p, c.c_uint, c.c_int, c.c_ulong]
    for symbol in symbols:
        code = desktop.x.XKeysymToKeycode(desktop.display, symbol)
        assert code
        desktop.input.XTestFakeKeyEvent(desktop.display, code, 1, 0)
        desktop.input.XTestFakeKeyEvent(desktop.display, code, 0, 0)
        desktop.x.XSync(desktop.display, 0)
        time.sleep(0.25)


def main():
    if (sys.platform != 'linux' or not Path('/.dockerenv').is_file()
            or os.environ.get('GPUI_ISOLATED_EDITOR_CONTAINER') != '1'
            or os.environ.get('GPUI_ISOLATED_VIRTUAL_DESKTOP') != '1'):
        raise RuntimeError('Requires an owned disposable editor container and virtual desktop')
    binary, output = [Path(a).resolve() for a in sys.argv[1:3]]
    output.mkdir(parents=True, exist_ok=True)
    (output/'verification.json').unlink(missing_ok=True)
    load('gpui-content-native-smoke.py').wait_window_manager()
    helper = load('gpui-copy-sound-smoke.py')
    editor = load('gpui-editor-native-smoke.py')
    pointer = load('gpui-library-native-smoke.py').pointer
    cases = []
    for mode, width in [('dark', 1600), ('light', 1600), ('dark', 1100)]:
        label = f'{mode}-{width}'
        with tempfile.TemporaryDirectory(prefix='industrialis-explorer-native-') as fresh:
            root = Path(fresh)
            profile = root/'profile'
            instance = profile/'instances/fixture'
            game = instance/'.minecraft'
            game.mkdir(parents=True)
            (instance/'mmc-pack.json').write_text('{}')
            (instance/'instance.json').write_text(json.dumps({'name': 'Explorer fixture', 'pack_version': 'fixture', 'acceptanceExtension': True}))
            original = '# Keep this comment\ngeneral {\n B:enabled=true\n I:count=42\n}\n'
            sources = {
                'config/angelica/options.json': '{"quality":true}',
                'config/GregTech/main.cfg': '# Main\ngeneral {\n B:enabled=true\n}\n',
                'config/nested/tweaks.cfg': original,
                'scripts/recipes.zs': '// Recipes\nprint("GTNH");\n',
                'resourcepacks/Modernity.zip': 'read-only asset',
                'options.txt': 'music:0.5\n', 'README.md': '# Instance files\n'}
            for rel, text in sources.items():
                path = game/rel
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text)
            (profile/'launcher-settings.json').write_text(json.dumps({'theme_mode': mode, 'theme_preset': 'sandstone', 'window_width': width, 'window_height': 900, 'launch_maximized': False, 'acceptanceExtension': True}))
            env = dict(os.environ, HOME=str(root/'home'), XDG_CONFIG_HOME=str(root/'config'), INDUSTRIALIS_DATA_DIR=str(profile))
            with (output/f'{label}.stdout.log').open('w') as stdout, (output/f'{label}.stderr.log').open('w') as stderr:
                app = subprocess.Popen([str(binary)], env=env, stdout=stdout, stderr=stderr)
                desktop = None
                try:
                    desktop = helper.VirtualDesktop(app)
                    deadline = time.monotonic()+40
                    while True:
                        try:
                            desktop.owned_window()
                            break
                        except AssertionError:
                            assert app.poll() is None and time.monotonic() < deadline
                            time.sleep(0.1)
                    time.sleep(1)
                    desktop.click(60, 205)
                    time.sleep(1)
                    desktop.click(1285 if width == 1600 else 885, 119)
                    time.sleep(1)
                    desktop.capture(output/f'{label}-root.png')
                    dx = 0 if width == 1600 else -270
                    desktop.click(940+dx, 271)
                    time.sleep(1)
                    desktop.capture(output/f'{label}-expanded.png')
                    desktop.click(955+dx, 350)
                    time.sleep(1)
                    desktop.click(975+dx, 378)
                    time.sleep(1)
                    desktop.capture(output/f'{label}-file.png')
                    source = game/'config/nested/tweaks.cfg'
                    paired = instance/'persistent-minecraft/config/nested/tweaks.cfg'
                    case = {'theme': mode, 'width': width, 'normalCloseExitCode': 0}
                    if width == 1600:
                        desktop.click(1195, 243)
                        time.sleep(0.5)
                        desktop.click(1370, 306)
                        time.sleep(0.5)
                        desktop.click(940, 435)
                        time.sleep(0.5)
                        assert source.read_text() == original and not paired.exists(), 'Folder expansion saved the draft'
                        desktop.capture(output/f'{label}-unsaved-folder-expansion.png')
                        desktop.click(1195, 410)
                        desktop.wait(lambda: paired.is_file() and 'B:enabled=false' in source.read_text(), 'Native Easy save did not complete')
                        assert paired.read_bytes() == source.read_bytes()
                        assert source.read_text().startswith('# Keep this comment')
                        desktop.capture(output/f'{label}-easy-saved.png')
                        desktop.click(1265, 243)
                        time.sleep(1)
                        desktop.click(1300, 350)
                        editor.keys(desktop, find=True)
                        editor.keys(desktop, text='enabled')
                        desktop.capture(output/f'{label}-advanced-find.png')
                        editor.keys(desktop, escape=True)
                        saved = source.read_bytes()
                        assert saved == paired.read_bytes(), 'Find changed the source'
                        desktop.click(940, 266)
                        tree_keys(desktop, [0xff53, 0xff54, 0xff53])
                        time.sleep(0.5)
                        desktop.capture(output/f'{label}-keyboard-tree.png')
                        desktop.click(1108, 207)
                        (game/'new.json').write_text('{"external":true}')
                        desktop.click(1080, 207)
                        time.sleep(1)
                        desktop.capture(output/f'{label}-refresh.png')
                        desktop.click(940, 238)
                        desktop.capture(output/f'{label}-root-collapsed.png')
                        desktop.click(940, 238)
                        border_before = desktop.pixel(1163, 600)
                        pointer(desktop, 1160, 430, 1, True)
                        pointer(desktop, 1210, 430)
                        pointer(desktop, 1210, 430, 1, False)
                        desktop.capture(output/f'{label}-resized.png')
                        assert desktop.pixel(1163, 600) != border_before, 'Native splitter did not move the editor boundary'
                        assert source.read_bytes() == saved and paired.read_bytes() == saved
                        case.update({'draftPreservedOnExpansion': True, 'easySavePairedBytes': True, 'findDidNotChangeBytes': True, 'savedSha256': hashlib.sha256(saved).hexdigest()})
                    assert json.loads((instance/'instance.json').read_text())['acceptanceExtension']
                    assert json.loads((profile/'launcher-settings.json').read_text())['acceptanceExtension']
                    desktop.windows.close_game(app.pid)
                    assert app.wait(timeout=15) == 0
                    if width == 1600:
                        assert source.read_bytes() == saved and paired.read_bytes() == saved, 'Normal close changed the source after Find'
                    cases.append(case)
                finally:
                    if desktop:
                        desktop.x.XCloseDisplay(desktop.display)
                    if app.poll() is None:
                        app.terminate()
                        app.wait(timeout=10)
    (output/'verification.json').write_text(json.dumps({'physicalDesktopUsed': False, 'originalUserProfileUsed': False, 'binarySha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'cases': cases}, indent=2))


if __name__ == '__main__':
    main()
