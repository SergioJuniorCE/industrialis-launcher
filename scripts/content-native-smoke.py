#!/usr/bin/env python3
"""Modernity content acceptance on an explicitly owned container desktop only."""
import os,sys,json,time,tempfile,subprocess
import ctypes as c
import hashlib,zipfile
from pathlib import Path
from importlib.util import spec_from_file_location,module_from_spec

def wait_window_manager():
    # Openbox is launched by the owned runner. Wait for its advertised readiness
    # before creating the app window, rather than racing its startup scan.
    x=c.CDLL('libX11.so.6');dsp=c.c_void_p;win=c.c_ulong
    x.XOpenDisplay.argtypes=[c.c_char_p];x.XOpenDisplay.restype=dsp
    x.XDefaultRootWindow.argtypes=[dsp];x.XDefaultRootWindow.restype=win
    x.XInternAtom.argtypes=[dsp,c.c_char_p,c.c_int];x.XInternAtom.restype=win
    x.XGetWindowProperty.argtypes=[dsp,win,win,c.c_long,c.c_long,c.c_int,win,c.POINTER(win),c.POINTER(c.c_int),c.POINTER(c.c_ulong),c.POINTER(c.c_ulong),c.POINTER(c.POINTER(c.c_ubyte))]
    x.XFree.argtypes=[c.c_void_p];x.XCloseDisplay.argtypes=[dsp]
    display=x.XOpenDisplay(None);assert display,'No owned virtual display'
    try:
        deadline=time.monotonic()+15
        while time.monotonic()<deadline:
            atom=x.XInternAtom(display,b'_NET_SUPPORTING_WM_CHECK',1)
            if atom:
                actual,fmt,count,left,data=win(),c.c_int(),c.c_ulong(),c.c_ulong(),c.POINTER(c.c_ubyte)()
                status=x.XGetWindowProperty(display,x.XDefaultRootWindow(display),atom,0,1,0,0,c.byref(actual),c.byref(fmt),c.byref(count),c.byref(left),c.byref(data))
                ready=status==0 and fmt.value==32 and count.value==1
                if data:x.XFree(data)
                if ready:return
            time.sleep(0.05)
        raise RuntimeError('Owned window manager did not become ready')
    finally:x.XCloseDisplay(display)

def main():
    if sys.platform != 'linux' or not Path('/.dockerenv').is_file() or os.environ.get('LAUNCHER_ISOLATED_CONTENT_CONTAINER') != '1' or os.environ.get('LAUNCHER_ISOLATED_VIRTUAL_DESKTOP') != '1':
        raise RuntimeError('Requires an explicitly owned disposable content container/desktop')
    binary,output=[Path(a).resolve() for a in sys.argv[1:3]]
    output.mkdir(parents=True,exist_ok=True)
    mode=sys.argv[3] if len(sys.argv)>3 else 'full'
    probe=mode=='probe'
    wait_window_manager()
    spec=spec_from_file_location('desktop',Path(__file__).with_name('copy-sound-smoke.py'));helper=module_from_spec(spec);spec.loader.exec_module(helper)
    with tempfile.TemporaryDirectory(prefix='industrialis-content-native-') as fresh:
        root=Path(fresh);profile=root/'profile';instance=profile/'instances/fixture'
        (instance/'.minecraft').mkdir(parents=True)
        (instance/'mmc-pack.json').write_text('{}')
        (instance/'instance.json').write_text(json.dumps({'name':'Content fixture','pack_version':'2.9.0-RC-2','acceptanceExtension':True}))
        (profile/'launcher-settings.json').write_text(json.dumps({'theme_mode':'dark','theme_preset':'sandstone','window_width':1600,'window_height':900,'launch_maximized':False,'acceptanceExtension':True}))
        env=dict(os.environ,HOME=str(root/'home'),XDG_CONFIG_HOME=str(root/'config'),INDUSTRIALIS_DATA_DIR=str(profile))
        with (output/'native.stdout.log').open('w') as stdout,(output/'native.stderr.log').open('w') as stderr:
            app=subprocess.Popen([str(binary)],env=env,stdout=stdout,stderr=stderr);desktop=None
            try:
                desktop=helper.VirtualDesktop(app);started=time.monotonic();deadline=started+40;activation_used=False
                while True:
                    try:desktop.owned_window();break
                    except AssertionError:
                        if app.poll() is not None:raise RuntimeError(f'Fixture app exited during startup: {app.returncode}')
                        if time.monotonic()-started>5 and not activation_used:
                            handoff=subprocess.run([str(binary)],env=env,stdout=stdout,stderr=stderr,timeout=15)
                            assert handoff.returncode==0,'Owned activation handoff failed'
                            activation_used=True
                        if time.monotonic()>=deadline:raise RuntimeError(f'Owned fixture startup timed out: {desktop.windows.close_game(app.pid,inspect_only=True)}')
                        time.sleep(0.1)
                time.sleep(2);desktop.click(60,215);time.sleep(1)
                desktop.capture(output/'selected.png')
                desktop.click(1407,119);time.sleep(1);desktop.capture(output/'add-ons.png')
                if not probe:
                    desktop.click(1005,202);time.sleep(1);desktop.capture(output/'resource-packs.png')
                    desktop.click(1200,456);time.sleep(8);desktop.capture(output/'catalog.png')
                    if mode != 'catalog':
                        desktop.click(1532,541)
                        deadline=time.monotonic()+120
                        while not list((instance/'.minecraft/resourcepacks').glob('Modernity-2.9.x-*.zip')):
                            assert time.monotonic()<deadline,'Main resource pack did not install';time.sleep(0.1)
                        main_pack=list((instance/'.minecraft/resourcepacks').glob('Modernity-2.9.x-*.zip'))[0]
                        desktop.wait(lambda:(instance/'persistent-minecraft/.industrialis-content/resourcepacks'/f'{main_pack.name}.json').is_file(),'Missing main pack receipt')
                        time.sleep(1);desktop.capture(output/'main-installed.png')
                        window,_,_,_,_=desktop.owned_window()
                        wx,wy,child=c.c_int(),c.c_int(),c.c_ulong()
                        assert desktop.x.XTranslateCoordinates(desktop.display,window,desktop.x.XDefaultRootWindow(desktop.display),0,0,c.byref(wx),c.byref(wy),c.byref(child))
                        desktop.input.XTestFakeMotionEvent(desktop.display,-1,wx.value+1200,wy.value+700,0)
                        for _ in range(24):
                            desktop.input.XTestFakeButtonEvent(desktop.display,5,1,0)
                            desktop.input.XTestFakeButtonEvent(desktop.display,5,0,0)
                        desktop.x.XSync(desktop.display,0);time.sleep(1);desktop.capture(output/'extras-bottom.png')
                        if mode=='full':
                            desktop.click(1534,650)
                            addon=instance/'.minecraft/resourcepacks/Modernity-GTNHAddon-OutlinedOres.zip'
                            time.sleep(1);desktop.capture(output/'after-addon-click.png')
                            try:desktop.wait(lambda:addon.is_file(),'Optional texture add-on was not installed',seconds=45)
                            except AssertionError:
                                desktop.capture(output/'addon-failure-bottom.png')
                                for _ in range(40):
                                    desktop.input.XTestFakeButtonEvent(desktop.display,4,1,0);desktop.input.XTestFakeButtonEvent(desktop.display,4,0,0)
                                desktop.x.XSync(desktop.display,0);time.sleep(1);desktop.capture(output/'addon-failure-top.png')
                                raise
                            desktop.wait(lambda:(instance/'persistent-minecraft/.industrialis-content/resourcepacks'/f'{addon.name}.json').is_file(),'Missing add-on receipt')
                            time.sleep(1)
                            for _ in range(24):
                                desktop.input.XTestFakeButtonEvent(desktop.display,5,1,0);desktop.input.XTestFakeButtonEvent(desktop.display,5,0,0)
                            desktop.x.XSync(desktop.display,0);time.sleep(1)
                            desktop.click(1534,781)
                            mod=instance/'.minecraft/mods/MyCTMLib-1.4.2.jar'
                            desktop.wait(lambda:mod.is_file(),'Optional MyCTMLib mod was not installed')
                            desktop.wait(lambda:(instance/'persistent-minecraft/.industrialis-content/mods'/f'{mod.name}.json').is_file(),'Missing mod receipt')
                            time.sleep(1);desktop.capture(output/'extras-installed.png')
                            desktop.click(910,202);time.sleep(1);desktop.capture(output/'mods-installed.png')
                            sources=[]
                            for kind,path in [('resourcepacks',main_pack),('resourcepacks',addon),('mods',mod)]:
                                assert (instance/'persistent-minecraft'/kind/path.name).read_bytes()==path.read_bytes()
                                source=json.loads((instance/'persistent-minecraft/.industrialis-content'/kind/f'{path.name}.json').read_text())
                                assert source['release']=='weekly-2.9.x-2026-10-12' and source['pack_series']=='2.9'
                                if kind=='resourcepacks':
                                    with zipfile.ZipFile(path) as z:assert json.loads(z.read('pack.mcmeta').decode('utf-8-sig'))['pack']['pack_format']==1
                                sources.append({'kind':kind,'filename':path.name,'installedSha256':hashlib.sha256(path.read_bytes()).hexdigest(),'source':source})
                            assert json.loads((instance/'instance.json').read_text())['acceptanceExtension']
                            assert json.loads((profile/'launcher-settings.json').read_text())['acceptanceExtension']
                            receipt={'startupActivationUsed':activation_used,'passed':True,'physicalDesktopUsed':False,'platform':'Linux x64 / owned private Xvfb','mainPackInstalled':True,'optionalPackInstalled':True,'optionalModInstalled':True,'pairedFilesMatch':True,'metadataPreserved':True,'binarySha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'artifacts':sources}

                desktop.windows.close_game(app.pid);assert app.wait(timeout=15)==0
                if mode=='full':
                    receipt['normalCloseExitCode']=0
                    (output/'native-verification.json').write_text(json.dumps(receipt,indent=2))
            finally:
                if desktop is not None:desktop.x.XCloseDisplay(desktop.display)
                if app.poll() is None:app.terminate();app.wait(timeout=10)

if __name__=='__main__':main()
