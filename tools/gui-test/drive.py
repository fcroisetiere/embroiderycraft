import sys, time, subprocess, os
from Xlib import X, display, XK
from Xlib.ext import xtest

d = display.Display(os.environ["DISPLAY"])

def move(x, y):
    xtest.fake_input(d, X.MotionNotify, x=x, y=y); d.sync(); time.sleep(0.05)
def down(): xtest.fake_input(d, X.ButtonPress, 1); d.sync(); time.sleep(0.05)
def up(): xtest.fake_input(d, X.ButtonRelease, 1); d.sync(); time.sleep(0.05)
def click(x, y): move(x, y); down(); up(); time.sleep(0.08)
def drag(x0, y0, x1, y1, steps=12):
    move(x0, y0); down()
    for i in range(1, steps + 1):
        move(x0 + (x1 - x0) * i // steps, y0 + (y1 - y0) * i // steps)
    up(); time.sleep(0.15)
def key(name, shift=False, ctrl=False):
    kc = d.keysym_to_keycode(XK.string_to_keysym(name))
    mods = []
    if ctrl: mods.append(d.keysym_to_keycode(XK.string_to_keysym("Control_L")))
    if shift: mods.append(d.keysym_to_keycode(XK.string_to_keysym("Shift_L")))
    for m in mods: xtest.fake_input(d, X.KeyPress, m)
    xtest.fake_input(d, X.KeyPress, kc); xtest.fake_input(d, X.KeyRelease, kc)
    for m in reversed(mods): xtest.fake_input(d, X.KeyRelease, m)
    d.sync(); time.sleep(0.2)
def shot(path): subprocess.run(["import", "-window", "root", path], check=True)

time.sleep(2.5)
def find(win):
    try:
        name = win.get_wm_name()
    except Exception:
        name = None
    if name and "EmbroideryCraft" in str(name):
        return win
    try:
        for c in win.query_tree().children:
            r = find(c)
            if r: return r
    except Exception:
        pass
    return None
w = find(d.screen().root)
print("window:", w)
if w:
    w.set_input_focus(X.RevertToParent, X.CurrentTime); d.sync()
move(700, 400); time.sleep(0.3)
# 1. rectangle with default Fill style
key("r"); drag(330, 150, 520, 260)
# 2. polyline closed by clicking the first point
key("p")
for (x, y) in [(620, 150), (780, 150), (780, 280)]: click(x, y)
click(621, 151)
# 3. freehand scribble becomes a Fill too; draw a zigzag open shape instead after switching to run style? keep fill
key("e"); drag(350, 330, 470, 430)
# 4. select tool: click the rectangle, then move it by dragging
key("v"); click(400, 200)
shot(sys.argv[1] + "_a.png")
drag(400, 200, 430, 215)
time.sleep(0.5)
shot(sys.argv[1] + "_b.png")
# 5. undo twice, redo once
key("z", ctrl=True); key("z", ctrl=True); key("z", ctrl=True, shift=True)
time.sleep(0.5)
shot(sys.argv[1] + "_c.png")

if len(sys.argv) > 2 and sys.argv[2] == "export":
    key("e", ctrl=True); time.sleep(1.0)
    shot(sys.argv[1] + "_dlg.png")
    key("Return"); time.sleep(1.0)
    shot(sys.argv[1] + "_after.png")
