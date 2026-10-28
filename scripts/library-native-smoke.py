"""Actual native drag previews in generated profiles on a private Xvfb."""
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


def pointer(desktop, x, y, button=None, down=None):
    window, _, _, width, height = desktop.owned_window()
    assert 0 <= x < width and 0 <= y < height
    rx, ry, child = c.c_int(), c.c_int(), c.c_ulong()
    assert desktop.x.XTranslateCoordinates(desktop.display, window, desktop.x.XDefaultRootWindow(desktop.display),
                                          0, 0, c.byref(rx), c.byref(ry), c.byref(child))
    desktop.x.XSetInputFocus(desktop.display, window, 1, 0)
    desktop.input.XTestFakeMotionEvent(desktop.display, -1, rx.value+x, ry.value+y, 0)
    desktop.x.XSync(desktop.display, 0)
    time.sleep(0.1)
    if button is not None:
        desktop.input.XTestFakeButtonEvent(desktop.display, button, int(down), 0)
        desktop.x.XSync(desktop.display, 0)
    time.sleep(0.4)


def main():
    if (sys.platform != 'linux' or not Path('/.dockerenv').is_file()
            or os.environ.get('LAUNCHER_ISOLATED_LIBRARY_CONTAINER') != '1'
            or os.environ.get('LAUNCHER_ISOLATED_VIRTUAL_DESKTOP') != '1'):
        raise RuntimeError('Requires an owned disposable library container/desktop')
    binary, output = [Path(a).resolve() for a in sys.argv[1:3]]
    probe = len(sys.argv)>3 and sys.argv[3]=='probe'
    output.mkdir(parents=True,exist_ok=True)
    if not probe: (output/'verification.json').unlink(missing_ok=True)
    spec=spec_from_file_location('desktop',Path(__file__).with_name('copy-sound-smoke.py'))
    helper=module_from_spec(spec)
    spec.loader.exec_module(helper)
    receipts=[]
    for mode in (['dark'] if probe else ['dark','light']):
        with tempfile.TemporaryDirectory(prefix='industrialis-library-native-') as fresh:
            root=Path(fresh)
            profile=root/'profile'
            for id in ['a','b','c']:
                instance=profile/'instances'/id
                (instance/'.minecraft').mkdir(parents=True)
                (instance/'mmc-pack.json').write_text('{}')
                (instance/'instance.json').write_text(json.dumps({'name':id.upper(),'pack_version':'fixture','acceptanceExtension':True}))
            groups=profile/'instances/instgroups.json'
            groups.write_text(json.dumps({'formatVersion':1,'groups':{'Main':{'instances':['a','b','c'],'color':'keep'}}}))
            (profile/'launcher-settings.json').write_text(json.dumps({'theme_mode':mode,'theme_preset':'sandstone',
                'window_width':1600,'window_height':900,'launch_maximized':False,'acceptanceExtension':True}))
            env=dict(os.environ,HOME=str(root/'home'),XDG_CONFIG_HOME=str(root/'config'),INDUSTRIALIS_DATA_DIR=str(profile))
            with (output/f'{mode}.stdout.log').open('w') as stdout,(output/f'{mode}.stderr.log').open('w') as stderr:
                app=subprocess.Popen([str(binary)],env=env,stdout=stdout,stderr=stderr)
                desktop=None
                try:
                    desktop=helper.VirtualDesktop(app)
                    deadline=time.monotonic()+60
                    while True:
                        try:
                            desktop.owned_window()
                            break
                        except AssertionError:
                            assert app.poll() is None and time.monotonic()<deadline
                            time.sleep(0.1)
                    time.sleep(2)
                    assert desktop.owned_window()[3:5]==(1600,900)
                    desktop.capture(output/f'{mode}-before.png')
                    if not probe:
                        pointer(desktop,395,225,1,True)
                        pointer(desktop,407,237)
                        pointer(desktop,40,215)
                        desktop.capture(output/f'{mode}-drag-preview.png')
                        assert json.loads(groups.read_text())['groups']['Main']['instances']==['a','b','c']
                        for _ in range(3): pointer(desktop,40,215)
                        pointer(desktop,40,215,1,False)
                        desktop.wait(lambda:json.loads(groups.read_text())['groups']['Main']['instances']==['c','a','b'],
                                     'Native drop did not persist the preview order')
                        assert json.loads(groups.read_text())['groups']['Main']['color']=='keep'
                        desktop.capture(output/f'{mode}-after-drop.png')
                        pointer(desktop,60,225,3,True)
                        pointer(desktop,60,225,3,False)
                        desktop.capture(output/f'{mode}-context-menu.png')
                        desktop.click(120,405)
                        # The owned fixture has no network. The manual action
                        # finishes with a longer status message for C, beyond
                        # the previous short "Groups saved" footer label.
                        desktop.wait(lambda:any(desktop.pixel(x,887)!=desktop.pixel(600,887)
                                     for x in range(190,330,3)), 'Manual menu check did not report completion')
                        desktop.capture(output/f'{mode}-manual-check.png')
                        assert json.loads((profile/'launcher-settings.json').read_text())['acceptanceExtension']
                        receipts.append({'mode':mode,'previewDidNotWrite':True,'reorderedIds':['c','a','b'],
                                         'manualCheckDispatched':True,
                                         'groupExtensionsPreserved':True,'unknownPreferencesPreserved':True})
                    desktop.windows.close_game(app.pid)
                    assert app.wait(timeout=15)==0
                finally:
                    if desktop is not None: desktop.x.XCloseDisplay(desktop.display)
                    if app.poll() is None:
                        app.terminate()
                        try: app.wait(timeout=5)
                        except subprocess.TimeoutExpired:
                            app.kill()
                            app.wait(timeout=5)
    if not probe:
        receipt={'passed':True,'platform':'Linux x64 / owned private Xvfb','physicalDesktopUsed':False,
                 'executableSha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'modes':receipts,
                 'normalCloseExitCodes':[0,0],'limits':'Network-disabled generated profiles; hourly timing uses the app fake-clock regression'}
        (output/'verification.json').write_text(json.dumps(receipt,indent=2)+'\n')
        print(json.dumps(receipt,indent=2))

if __name__=='__main__': main()
