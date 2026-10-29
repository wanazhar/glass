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
    let click_button = (action == "pointer-click")
        .then(|| {
            button_id(value["button"].as_str().unwrap_or_default())
                .ok_or(InputError::Invalid("button must be left, middle, or right"))
        })
        .transpose()?;
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
        "pointer-move" | "pointer-button" | "pointer-click" | "scroll" => {
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
            } else if let Some(button) = click_button {
                unsafe {
                    (loaded.button)(xdisplay, button as c_uint, 1, 0);
                    (loaded.button)(xdisplay, button as c_uint, 0, 0);
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
        "key-tap" => {
            let key = value["key"]
                .as_str()
                .ok_or(InputError::Invalid("key must be a string"))?;
            let key = key_name(key).ok_or(InputError::Invalid("key is not supported"))?;
            tap_key(&loaded, xdisplay, key)?;
        }
        "key-chord" => {
            let keys = value["keys"]
                .as_array()
                .ok_or(InputError::Invalid("keys must be an array"))?;
            let names = validate_key_chord(keys)?;
            let modifiers = &names[..names.len() - 1];
            let target = names[names.len() - 1];
            let mut pressed = Vec::new();
            for modifier in modifiers {
                if let Err(error) = send_key(&loaded, xdisplay, modifier, true) {
                    for held in pressed.into_iter().rev() {
                        let _ = send_key(&loaded, xdisplay, held, false);
                    }
                    return Err(error);
                }
                pressed.push(*modifier);
            }
            let target_result = tap_key(&loaded, xdisplay, target);
            let mut release_error = None;
            for modifier in pressed.into_iter().rev() {
                if let Err(error) = send_key(&loaded, xdisplay, modifier, false) {
                    release_error.get_or_insert(error);
                }
            }
            target_result?;
            if let Some(error) = release_error {
                return Err(error);
            }
        }
        "text" => {
            let text = value["text"]
                .as_str()
                .ok_or(InputError::Invalid("text must be a string"))?;
            validate_text(text)?;
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
                if character.is_ascii() {
                    send_character(&loaded, xdisplay, character)?;
                } else {
                    send_unicode_character(&loaded, xdisplay, character)?;
                }
            }
        }
        _ => {
            return Err(InputError::Invalid(
                "action must be pointer-move, pointer-button, pointer-click, key, text, or scroll",
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
    for (index, pair) in hex.as_bytes().as_chunks::<2>().0.iter().enumerate() {
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

/// Releases any XTest state left in the isolated server if its prior Gateway
/// exited before it could persist/release its local held-key bookkeeping.
pub(super) fn release_all(display: &str, cookie: &[u8; 16]) -> Result<(), InputError> {
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

    let mut keymap = [0 as c_char; 32];
    if unsafe { (loaded.query_keymap)(xdisplay, keymap.as_mut_ptr()) } == 0 {
        return Err(InputError::Display(
            "Could not inspect managed display key state".into(),
        ));
    }
    for keycode in 8..=255u16 {
        let byte = keymap[(keycode / 8) as usize];
        if byte & (1 << (keycode % 8)) != 0 {
            unsafe {
                (loaded.key)(xdisplay, keycode as c_uint, 0, 0);
            }
        }
    }

    let root = unsafe { (loaded.default_root_window)(xdisplay) };
    let mut root_return = 0;
    let mut child_return = 0;
    let mut root_x = 0;
    let mut root_y = 0;
    let mut win_x = 0;
    let mut win_y = 0;
    let mut mask = 0;
    if unsafe {
        (loaded.query_pointer)(
            xdisplay,
            root,
            &mut root_return,
            &mut child_return,
            &mut root_x,
            &mut root_y,
            &mut win_x,
            &mut win_y,
            &mut mask,
        )
    } == 0
    {
        return Err(InputError::Display(
            "Could not inspect managed display pointer state".into(),
        ));
    }
    for button in 1..=5u32 {
        let button_mask = 1u32 << (7 + button);
        if mask & button_mask != 0 {
            unsafe {
                (loaded.button)(xdisplay, button, 0, 0);
            }
        }
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

fn validate_key_chord(keys: &[Value]) -> Result<Vec<&str>, InputError> {
    if !(2..=4).contains(&keys.len()) {
        return Err(InputError::Invalid("key chord must contain 2 to 4 keys"));
    }
    let mut names = Vec::with_capacity(keys.len());
    for (index, key) in keys.iter().enumerate() {
        let key = key
            .as_str()
            .ok_or(InputError::Invalid("chord keys must be strings"))?;
        let name = key_name(key).ok_or(InputError::Invalid("key is not supported"))?;
        let modifier = matches!(key, "Control" | "Alt" | "Shift" | "Meta");
        if modifier != (index + 1 < keys.len()) {
            return Err(InputError::Invalid(
                "key chord must list modifiers first and one non-modifier key last",
            ));
        }
        if names.contains(&name) {
            return Err(InputError::Invalid("key chord cannot repeat a key"));
        }
        names.push(name);
    }
    Ok(names)
}

fn tap_key(api: &XTest, display: *mut c_void, key: &str) -> Result<(), InputError> {
    send_key(api, display, key, true)?;
    if let Err(error) = send_key(api, display, key, false) {
        let _ = send_key(api, display, key, false);
        return Err(error);
    }
    Ok(())
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
        "F1" => "F1",
        "F2" => "F2",
        "F3" => "F3",
        "F4" => "F4",
        "F5" => "F5",
        "F6" => "F6",
        "F7" => "F7",
        "F8" => "F8",
        "F9" => "F9",
        "F10" => "F10",
        "F11" => "F11",
        "F12" => "F12",
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

/// GTK/XIM's Unicode compose entry lets XTest type codepoints without adding
/// a clipboard helper or changing the managed display's keymap.
fn send_unicode_character(
    api: &XTest,
    display: *mut c_void,
    character: char,
) -> Result<(), InputError> {
    send_key(api, display, "Control_L", true)?;
    if let Err(error) = send_key(api, display, "Shift_L", true) {
        let _ = send_key(api, display, "Control_L", false);
        return Err(error);
    }
    let start_compose = send_character(api, display, 'u');
    let release_shift = send_key(api, display, "Shift_L", false);
    let release_control = send_key(api, display, "Control_L", false);
    start_compose?;
    release_shift?;
    release_control?;
    for digit in unicode_hex(character).chars() {
        send_character(api, display, digit)?;
    }
    send_key(api, display, "space", true)?;
    send_key(api, display, "space", false)
}

fn validate_text(text: &str) -> Result<(), InputError> {
    if text.len() > 512
        || text
            .chars()
            .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\t'))
    {
        return Err(InputError::Invalid(
            "text must be at most 512 UTF-8 bytes without control characters except newline and tab",
        ));
    }
    Ok(())
}

fn unicode_hex(character: char) -> String {
    format!("{:x}", character as u32)
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
    query_keymap: unsafe extern "C" fn(*mut c_void, *mut c_char) -> c_int,
    default_root_window: unsafe extern "C" fn(*mut c_void) -> c_ulong,
    query_pointer: unsafe extern "C" fn(
        *mut c_void,
        c_ulong,
        *mut c_ulong,
        *mut c_ulong,
        *mut c_int,
        *mut c_int,
        *mut c_int,
        *mut c_int,
        *mut c_uint,
    ) -> c_int,
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
                query_keymap: load_symbol(x11, c"XQueryKeymap")?,
                default_root_window: load_symbol(x11, c"XDefaultRootWindow")?,
                query_pointer: load_symbol(x11, c"XQueryPointer")?,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn function_keys_are_supported_by_managed_input() {
        assert_eq!(key_name("F2"), Some("F2"));
        assert_eq!(key_name("F12"), Some("F12"));
        assert_eq!(key_name("F13"), None);
    }

    #[test]
    fn key_chords_require_supported_modifiers_then_one_key() {
        assert_eq!(
            validate_key_chord(&[json!("Alt"), json!("F2")]).unwrap(),
            vec!["Alt_L", "F2"]
        );
        assert_eq!(
            validate_key_chord(&[json!("Control"), json!("Shift"), json!("S")]).unwrap(),
            vec!["Control_L", "Shift_L", "S"]
        );
        assert!(validate_key_chord(&[]).is_err());
        assert!(validate_key_chord(&[json!("Alt")]).is_err());
        assert!(validate_key_chord(&[json!("F2"), json!("Alt")]).is_err());
        assert!(validate_key_chord(&[json!("Alt"), json!("Alt")]).is_err());
        assert!(validate_key_chord(&[json!("Alt"), json!("Unknown")]).is_err());
    }

    #[test]
    fn pointer_click_accepts_supported_button_and_normalized_coordinates() {
        let value = json!({"button":"left","x":0.5,"y":0.25});
        assert_eq!(button_id(value["button"].as_str().unwrap()), Some(1));
        assert_eq!(coordinates(&value).unwrap(), (640, 200));
    }

    #[test]
    fn pointer_click_rejects_unsupported_button_and_out_of_range_coordinates() {
        assert_eq!(button_id("wheel"), None);
        assert!(coordinates(&json!({"x":1.01,"y":0.5})).is_err());
    }

    #[test]
    fn bounded_text_accepts_utf8_and_newlines() {
        assert!(validate_text("Crème brûlée — 東京\nSecond line").is_ok());
        assert!(validate_text(&"é".repeat(256)).is_ok());
        assert!(validate_text(&"é".repeat(257)).is_err());
        assert!(validate_text("a\0b").is_err());
        assert!(validate_text("a\tb\nc").is_ok());
    }

    #[test]
    fn unicode_compose_uses_unpadded_lowercase_hex_codepoints() {
        assert_eq!(unicode_hex('é'), "e9");
        assert_eq!(unicode_hex('🐢'), "1f422");
    }

    #[test]
    fn release_all_clears_pressed_xtest_keys_and_buttons() {
        if std::env::var_os("GLASS_DESKTOP_RELEASE_ALL_E2E").as_deref()
            != Some(std::ffi::OsStr::new("1"))
        {
            return;
        }
        let root = std::env::temp_dir().join(format!(
            "glass-desktop-release-all-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let workspace = root.join("workspace");
        let sessions = root.join("sessions");
        std::fs::create_dir_all(&workspace).unwrap();
        let session = match super::super::process::start_session_with_environment(
            &workspace,
            &sessions,
            super::super::process::Environment::Xfce,
            super::super::process::Lifecycle::Disposable,
            None,
        ) {
            Ok(session) => session,
            Err(error) => {
                let _ = std::fs::remove_dir_all(&root);
                panic!("managed XFCE session did not start: {error}");
            }
        };
        inject(
            &session.display,
            &session.auth_cookie,
            "key",
            &json!({"key":"a","pressed":true}),
        )
        .unwrap();
        inject(
            &session.display,
            &session.auth_cookie,
            "pointer-button",
            &json!({"button":"left","pressed":true,"x":0.5,"y":0.5}),
        )
        .unwrap();
        release_all(&session.display, &session.auth_cookie).unwrap();

        let api = XTest::load().unwrap();
        let display_name = CString::new(session.display.as_str()).unwrap();
        let xdisplay =
            unsafe { api.open_authorized_display(display_name.as_ptr(), &session.auth_cookie) };
        assert!(!xdisplay.is_null());
        let close = DisplayGuard {
            display: xdisplay,
            close: api.close_display,
        };
        let keycode =
            unsafe { (api.keysym_to_keycode)(xdisplay, (api.string_to_keysym)(c"a".as_ptr())) };
        let mut keymap = [0 as c_char; 32];
        assert_ne!(
            unsafe { (api.query_keymap)(xdisplay, keymap.as_mut_ptr()) },
            0
        );
        let key_down = keymap[keycode as usize / 8] as u8 & (1 << (keycode % 8)) != 0;

        let root_window = unsafe { (api.default_root_window)(xdisplay) };
        let mut root_return = 0;
        let mut child_return = 0;
        let mut root_x = 0;
        let mut root_y = 0;
        let mut win_x = 0;
        let mut win_y = 0;
        let mut mask = 0;
        assert_ne!(
            unsafe {
                (api.query_pointer)(
                    xdisplay,
                    root_window,
                    &mut root_return,
                    &mut child_return,
                    &mut root_x,
                    &mut root_y,
                    &mut win_x,
                    &mut win_y,
                    &mut mask,
                )
            },
            0
        );
        let button_down = mask & (1 << 8) != 0;
        drop(close);
        drop(session);
        std::fs::remove_dir_all(&root).unwrap();
        assert!(!key_down, "release_all left an XTest key pressed");
        assert!(!button_down, "release_all left an XTest button pressed");
    }
}
