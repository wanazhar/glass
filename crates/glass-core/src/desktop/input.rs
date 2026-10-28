use serde_json::Value;
use std::{
    collections::HashSet,
    ffi::{CString, c_char, c_int, c_uint, c_ulong, c_void},
    sync::Mutex,
};

use super::{HEIGHT, InputError, WIDTH};

pub(super) fn inject(
    display: &str,
    cookie: &[u8; 16],
    action: &str,
    value: &Value,
) -> Result<(), InputError> {
    let loaded = XTest::load()?;
    let display_name = CString::new(display).map_err(|_| InputError::Invalid("Invalid display"))?;
    let _auth_lock = AUTH_LOCK.lock().map_err(|_| InputError::Unavailable)?;
    let xdisplay = unsafe { loaded.open_authorized_display(display_name.as_ptr(), cookie) };
    if xdisplay.is_null() {
        return Err(InputError::Display(
            "Could not open the managed display".into(),
        ));
    }
    let _close = DisplayGuard {
        display: xdisplay,
        close: loaded.close_display,
    };
    match action {
        "pointer-move" | "pointer-button" | "scroll" => {
            let button_event = if action == "pointer-button" {
                Some((
                    button_id(value["button"].as_str().unwrap_or_default())
                        .ok_or(InputError::Invalid("button must be left, middle, or right"))?,
                    value["pressed"]
                        .as_bool()
                        .ok_or(InputError::Invalid("pressed must be a boolean"))?,
                ))
            } else {
                None
            };
            if action != "pointer-button" || value.get("x").is_some() || value.get("y").is_some() {
                let (x, y) = coordinates(value)?;
                unsafe {
                    (loaded.motion)(xdisplay, 0, x, y, 0);
                }
            }
            if let Some((button, pressed)) = button_event {
                unsafe {
                    (loaded.button)(xdisplay, button as c_uint, if pressed { 1 } else { 0 }, 0);
                }
            } else if action == "scroll" {
                let dx = bounded_delta(value.get("deltaX").and_then(Value::as_f64).unwrap_or(0.0));
                let dy = bounded_delta(value.get("deltaY").and_then(Value::as_f64).unwrap_or(0.0));
                for _ in 0..dx.unsigned_abs() {
                    unsafe {
                        (loaded.button)(xdisplay, if dx < 0 { 6 } else { 7 }, 1, 0);
                        (loaded.button)(xdisplay, if dx < 0 { 6 } else { 7 }, 0, 0);
                    }
                }
                for _ in 0..dy.unsigned_abs() {
                    unsafe {
                        (loaded.button)(xdisplay, if dy < 0 { 4 } else { 5 }, 1, 0);
                        (loaded.button)(xdisplay, if dy < 0 { 4 } else { 5 }, 0, 0);
                    }
                }
            }
        }
        "key" => {
            let key = value["key"]
                .as_str()
                .ok_or(InputError::Invalid("key must be a string"))?;
            let pressed = value["pressed"]
                .as_bool()
                .ok_or(InputError::Invalid("pressed must be a boolean"))?;
            let key = key_name(key).ok_or(InputError::Invalid("key is not supported"))?;
            send_key(&loaded, xdisplay, key, pressed)?;
        }
        "text" => {
            let text = value["text"]
                .as_str()
                .ok_or(InputError::Invalid("text must be a string"))?;
            if text.len() > 512
                || !text.is_ascii()
                || text
                    .chars()
                    .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\t'))
            {
                return Err(InputError::Invalid(
                    "text must be at most 512 printable ASCII bytes",
                ));
            }
            for character in text.chars() {
                if character == '\n' {
                    send_key(&loaded, xdisplay, "Return", true)?;
                    send_key(&loaded, xdisplay, "Return", false)?;
                    continue;
                }
                if character == '\t' {
                    send_key(&loaded, xdisplay, "Tab", true)?;
                    send_key(&loaded, xdisplay, "Tab", false)?;
                    continue;
                }
                send_character(&loaded, xdisplay, character)?;
            }
        }
        _ => {
            return Err(InputError::Invalid(
                "action must be pointer-move, pointer-button, key, text, or scroll",
            ));
        }
    }
    unsafe {
        (loaded.flush)(xdisplay);
    }
    Ok(())
}

pub(super) fn decode_cookie(hex: &str) -> Result<[u8; 16], String> {
    if hex.len() != 32 {
        return Err("Invalid managed desktop cookie".into());
    }
    let digit = |value: u8| match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    };
    let mut bytes = [0u8; 16];
    for (index, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = digit(pair[0]).ok_or("Invalid managed desktop cookie")? * 16
            + digit(pair[1]).ok_or("Invalid managed desktop cookie")?;
    }
    Ok(bytes)
}

pub(super) fn button_id(button: &str) -> Option<u8> {
    match button {
        "left" => Some(1),
        "middle" => Some(2),
        "right" => Some(3),
        _ => None,
    }
}

pub(super) fn release_held(
    display: &str,
    cookie: &[u8; 16],
    buttons: &HashSet<u8>,
    keys: &HashSet<String>,
) -> Result<(), InputError> {
    if buttons.is_empty() && keys.is_empty() {
        return Ok(());
    }
    let loaded = XTest::load()?;
    let display_name = CString::new(display).map_err(|_| InputError::Invalid("Invalid display"))?;
    let _auth_lock = AUTH_LOCK.lock().map_err(|_| InputError::Unavailable)?;
    let xdisplay = unsafe { loaded.open_authorized_display(display_name.as_ptr(), cookie) };
    if xdisplay.is_null() {
        return Err(InputError::Display(
            "Could not open the managed display".into(),
        ));
    }
    let _close = DisplayGuard {
        display: xdisplay,
        close: loaded.close_display,
    };
    for button in buttons {
        unsafe {
            (loaded.button)(xdisplay, *button as c_uint, 0, 0);
        }
    }
    for key in keys {
        let Some(name) = key_name(key) else { continue };
        send_key(&loaded, xdisplay, name, false)?;
    }
    unsafe {
        (loaded.flush)(xdisplay);
    }
    Ok(())
}

pub(super) fn coordinates(value: &Value) -> Result<(c_int, c_int), InputError> {
    let x = value
        .get("x")
        .and_then(Value::as_f64)
        .ok_or(InputError::Invalid("x must be a normalized number"))?;
    let y = value
        .get("y")
        .and_then(Value::as_f64)
        .ok_or(InputError::Invalid("y must be a normalized number"))?;
    if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
        return Err(InputError::Invalid("x and y must be between 0 and 1"));
    }
    Ok((
        (x * (WIDTH - 1) as f64).round() as c_int,
        (y * (HEIGHT - 1) as f64).round() as c_int,
    ))
}

pub(super) fn bounded_delta(delta: f64) -> i64 {
    delta.round().clamp(-20.0, 20.0) as i64
}

fn key_name(key: &str) -> Option<&str> {
    Some(match key {
        "Enter" => "Return",
        "Escape" => "Escape",
        "Tab" => "Tab",
        "BackSpace" => "BackSpace",
        "Delete" => "Delete",
        "Home" => "Home",
        "End" => "End",
        "PageUp" => "Page_Up",
        "PageDown" => "Page_Down",
        "ArrowUp" => "Up",
        "ArrowDown" => "Down",
        "ArrowLeft" => "Left",
        "ArrowRight" => "Right",
        "Shift" => "Shift_L",
        "Control" => "Control_L",
        "Alt" => "Alt_L",
        "Meta" => "Super_L",
        "Space" => "space",
        _ if key.len() == 1 && key.as_bytes()[0].is_ascii_alphanumeric() => key,
        _ => return None,
    })
}

fn send_character(api: &XTest, display: *mut c_void, character: char) -> Result<(), InputError> {
    let mut shifted = false;
    let name = match character {
        '!' => {
            shifted = true;
            "1"
        }
        '@' => {
            shifted = true;
            "2"
        }
        '#' => {
            shifted = true;
            "3"
        }
        '$' => {
            shifted = true;
            "4"
        }
        '%' => {
            shifted = true;
            "5"
        }
        '^' => {
            shifted = true;
            "6"
        }
        '&' => {
            shifted = true;
            "7"
        }
        '*' => {
            shifted = true;
            "8"
        }
        '(' => {
            shifted = true;
            "9"
        }
        ')' => {
            shifted = true;
            "0"
        }
        '_' => {
            shifted = true;
            "minus"
        }
        '+' => {
            shifted = true;
            "equal"
        }
        '{' => {
            shifted = true;
            "bracketleft"
        }
        '}' => {
            shifted = true;
            "bracketright"
        }
        '|' => {
            shifted = true;
            "backslash"
        }
        ':' => {
            shifted = true;
            "semicolon"
        }
        '"' => {
            shifted = true;
            "apostrophe"
        }
        '<' => {
            shifted = true;
            "comma"
        }
        '>' => {
            shifted = true;
            "period"
        }
        '?' => {
            shifted = true;
            "slash"
        }
        '~' => {
            shifted = true;
            "grave"
        }
        ' ' => "space",
        '-' => "minus",
        '=' => "equal",
        '[' => "bracketleft",
        ']' => "bracketright",
        '\\' => "backslash",
        ';' => "semicolon",
        '\'' => "apostrophe",
        ',' => "comma",
        '.' => "period",
        '/' => "slash",
        '`' => "grave",
        c if c.is_ascii_alphabetic() => {
            shifted = c.is_ascii_uppercase();
            if shifted { "Shift_L" } else { "" }
        }
        c if c.is_ascii_digit() => {
            return send_named_character(api, display, &c.to_string(), false);
        }
        _ => {
            return Err(InputError::Invalid(
                "text contains an unsupported character",
            ));
        }
    };
    let actual = if character.is_ascii_alphabetic() {
        character.to_ascii_lowercase().to_string()
    } else {
        name.to_owned()
    };
    send_named_character(api, display, &actual, shifted)
}

fn send_named_character(
    api: &XTest,
    display: *mut c_void,
    name: &str,
    shifted: bool,
) -> Result<(), InputError> {
    let name = CString::new(name).map_err(|_| InputError::Invalid("Invalid key"))?;
    let keysym = unsafe { (api.string_to_keysym)(name.as_ptr()) };
    let code = unsafe { (api.keysym_to_keycode)(display, keysym) };
    if code == 0 {
        return Err(InputError::Invalid("Key is unavailable on this display"));
    }
    let shift =
        unsafe { (api.keysym_to_keycode)(display, (api.string_to_keysym)(c"Shift_L".as_ptr())) };
    if shifted {
        unsafe {
            (api.key)(display, shift as c_uint, 1, 0);
        }
    }
    unsafe {
        (api.key)(display, code as c_uint, 1, 0);
        (api.key)(display, code as c_uint, 0, 0);
    }
    if shifted {
        unsafe {
            (api.key)(display, shift as c_uint, 0, 0);
        }
    }
    Ok(())
}

fn send_key(
    api: &XTest,
    display: *mut c_void,
    name: &str,
    pressed: bool,
) -> Result<(), InputError> {
    let name = CString::new(name).map_err(|_| InputError::Invalid("Invalid key"))?;
    let keysym = unsafe { (api.string_to_keysym)(name.as_ptr()) };
    let code = unsafe { (api.keysym_to_keycode)(display, keysym) };
    if code == 0 {
        return Err(InputError::Invalid("Key is unavailable on this display"));
    }
    unsafe {
        (api.key)(display, code as c_uint, if pressed { 1 } else { 0 }, 0);
    }
    Ok(())
}

static AUTH_LOCK: Mutex<()> = Mutex::new(());

struct XTest {
    _x11: *mut c_void,
    _xtst: *mut c_void,
    open_display: unsafe extern "C" fn(*const c_char) -> *mut c_void,
    close_display: unsafe extern "C" fn(*mut c_void) -> c_int,
    flush: unsafe extern "C" fn(*mut c_void) -> c_int,
    keysym_to_keycode: unsafe extern "C" fn(*mut c_void, c_ulong) -> u8,
    string_to_keysym: unsafe extern "C" fn(*const c_char) -> c_ulong,
    set_authorization: unsafe extern "C" fn(*const c_char, c_int, *const c_char, c_int),
    motion: unsafe extern "C" fn(*mut c_void, c_int, c_int, c_int, c_ulong) -> c_int,
    button: unsafe extern "C" fn(*mut c_void, c_uint, c_int, c_ulong) -> c_int,
    key: unsafe extern "C" fn(*mut c_void, c_uint, c_int, c_ulong) -> c_int,
}

impl XTest {
    fn load() -> Result<Self, InputError> {
        unsafe {
            let x11 = libc::dlopen(c"libX11.so.6".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
            let xtst = libc::dlopen(c"libXtst.so.6".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
            if x11.is_null() || xtst.is_null() {
                return Err(InputError::Display(
                    "X11 input support is unavailable".into(),
                ));
            }
            Ok(Self {
                _x11: x11,
                _xtst: xtst,
                open_display: load_symbol(x11, c"XOpenDisplay")?,
                close_display: load_symbol(x11, c"XCloseDisplay")?,
                flush: load_symbol(x11, c"XFlush")?,
                keysym_to_keycode: load_symbol(x11, c"XKeysymToKeycode")?,
                string_to_keysym: load_symbol(x11, c"XStringToKeysym")?,
                set_authorization: load_symbol(x11, c"XSetAuthorization")?,
                motion: load_symbol(xtst, c"XTestFakeMotionEvent")?,
                button: load_symbol(xtst, c"XTestFakeButtonEvent")?,
                key: load_symbol(xtst, c"XTestFakeKeyEvent")?,
            })
        }
    }

    unsafe fn open_authorized_display(
        &self,
        name: *const c_char,
        cookie: &[u8; 16],
    ) -> *mut c_void {
        let protocol = c"MIT-MAGIC-COOKIE-1";
        unsafe {
            (self.set_authorization)(
                protocol.as_ptr(),
                protocol.to_bytes().len() as c_int,
                cookie.as_ptr().cast(),
                cookie.len() as c_int,
            );
            let display = (self.open_display)(name);
            (self.set_authorization)(
                protocol.as_ptr(),
                protocol.to_bytes().len() as c_int,
                std::ptr::null(),
                0,
            );
            display
        }
    }
}

impl Drop for XTest {
    fn drop(&mut self) {
        unsafe {
            libc::dlclose(self._xtst);
            libc::dlclose(self._x11);
        }
    }
}

unsafe fn load_symbol<T: Copy>(
    library: *mut c_void,
    name: &std::ffi::CStr,
) -> Result<T, InputError> {
    let symbol = unsafe { libc::dlsym(library, name.as_ptr()) };
    if symbol.is_null() {
        return Err(InputError::Display(
            "X11 input function is unavailable".into(),
        ));
    }
    Ok(unsafe { std::mem::transmute_copy(&symbol) })
}

struct DisplayGuard {
    display: *mut c_void,
    close: unsafe extern "C" fn(*mut c_void) -> c_int,
}

impl Drop for DisplayGuard {
    fn drop(&mut self) {
        unsafe {
            (self.close)(self.display);
        }
    }
}
