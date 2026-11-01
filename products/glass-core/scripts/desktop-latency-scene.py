#!/usr/bin/env python3
"""Private-Xvfb benchmark scene: motion changes a known pixel for true input latency."""

import ctypes as c
import ctypes.util
import sys
from pathlib import Path

W, H = 1280, 800
x = c.CDLL(ctypes.util.find_library("X11"))


def function(name, result, *arguments):
    f = getattr(x, name)
    f.restype = result
    f.argtypes = arguments
    return f


ptr = c.c_void_p
ulong = c.c_ulong
integer = c.c_int
open_display = function("XOpenDisplay", ptr, c.c_char_p)
display = open_display(None)
if not display:
    raise SystemExit("Private benchmark display unavailable")
screen = function("XDefaultScreen", integer, ptr)(display)
root = function("XRootWindow", ulong, ptr, integer)(display, screen)
visual = function("XDefaultVisual", ptr, ptr, integer)(display, screen)
depth = function("XDefaultDepth", integer, ptr, integer)(display, screen)
window = function(
    "XCreateSimpleWindow",
    ulong,
    ptr,
    ulong,
    integer,
    integer,
    c.c_uint,
    c.c_uint,
    c.c_uint,
    ulong,
    ulong,
)(display, root, 0, 0, W, H, 0, 0, 0)


class Attributes(c.Structure):
    _fields_ = [
        ("background_pixmap", ulong),
        ("background_pixel", ulong),
        ("border_pixmap", ulong),
        ("border_pixel", ulong),
        ("bit_gravity", integer),
        ("win_gravity", integer),
        ("backing_store", integer),
        ("backing_planes", ulong),
        ("backing_pixel", ulong),
        ("save_under", integer),
        ("event_mask", c.c_long),
        ("do_not_propagate_mask", c.c_long),
        ("override_redirect", integer),
        ("colormap", ulong),
        ("cursor", ulong),
    ]


attributes = Attributes()
attributes.override_redirect = 1
function("XChangeWindowAttributes", integer, ptr, ulong, ulong, c.POINTER(Attributes))(
    display, window, 1 << 9, c.byref(attributes)
)
function("XSelectInput", integer, ptr, ulong, c.c_long)(
    display, window, (1 << 15) | (1 << 6) | (1 << 2) | 1
)
function("XMapRaised", integer, ptr, ulong)(display, window)
function("XSetInputFocus", integer, ptr, ulong, integer, ulong)(display, window, 1, 0)
gc = function("XCreateGC", ptr, ptr, ulong, ulong, ptr)(display, window, 0, None)
pixels = bytearray(W * H * 4)
for row in range(H):
    for col in range(W):
        offset = (row * W + col) * 4
        noise = ((col * 17 + row * 31) ^ (col * row)) & 63
        pixels[offset : offset + 4] = bytes(
            (
                40 + (col // 8 + noise) % 100,
                30 + (row // 5 + noise) % 100,
                20 + noise,
                0,
            )
        )
buffer = c.create_string_buffer(bytes(pixels))
image = function(
    "XCreateImage",
    ptr,
    ptr,
    ptr,
    c.c_uint,
    integer,
    integer,
    ptr,
    c.c_uint,
    c.c_uint,
    integer,
    integer,
)(display, visual, depth, 2, 0, c.cast(buffer, ptr), W, H, 32, W * 4)
put = function(
    "XPutImage",
    integer,
    ptr,
    ulong,
    ptr,
    ptr,
    integer,
    integer,
    integer,
    integer,
    c.c_uint,
    c.c_uint,
)
foreground = function("XSetForeground", integer, ptr, ptr, ulong)
rectangle = function(
    "XFillRectangle", integer, ptr, ulong, ptr, integer, integer, c.c_uint, c.c_uint
)
text = function(
    "XDrawString", integer, ptr, ulong, ptr, integer, integer, c.c_char_p, integer
)
flush = function("XFlush", integer, ptr)
counter = 0
ready = False


def draw_marker():
    color = (
        (((counter * 73) & 255) << 16)
        | (((counter * 151) & 255) << 8)
        | ((counter * 199) & 255)
    )
    foreground(display, gc, color)
    rectangle(display, window, gc, 64, 64, 64, 64)
    foreground(display, gc, 0xFFFFFF)
    label = b"Glass desktop transport: pointer movement changes the square at top left"
    text(display, window, gc, 80, 32, label, len(label))
    flush(display)


event = c.create_string_buffer(192)
next_event = function("XNextEvent", integer, ptr, ptr)
while True:
    next_event(display, c.byref(event))
    kind = c.cast(event, c.POINTER(integer))[0]
    if kind == 12:
        put(display, window, gc, image, 0, 0, 0, 0, W, H)
        draw_marker()
        if not ready and len(sys.argv) > 1:
            function("XSync", integer, ptr, integer)(display, 0)
            Path(sys.argv[1]).write_text("ready")
            ready = True
    elif kind in (2, 4, 6):
        counter += 1
        draw_marker()
