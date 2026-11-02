// Mogged for Horizon: switch between your account skin and two custom presets
// with one key, with the same head-to-feet sweep as the Fabric mod. Other
// players who have the mod see it too, through the server's Horizon mod
// messages.
//
// This file is the mod's logic and state, with no game types in it: the
// bundle adds it to the game as `game::mogged`, and Cargo builds it as a
// library so its tests run on their own (`cargo test`). Everything that
// touches the game lives in `glue.rs`.
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// The mod's id: its row in Options > Mods and its key's line.
pub const ID: &str = "mogged";
pub const VERSION: &str = "1.1.0";
pub const DESCRIPTION: &str = "Switch between your account skin and two custom presets with one key and a smooth animation. Players with the mod see it too.";
/// The switch key's name, label and default.
pub const KEY: &str = "switch";
pub const KEY_LABEL: &str = "Switch preset";
pub const DEFAULT_KEY: &str = "key.keyboard.k";

/// Mod message channels (`mod_id:name`).
pub const CH_SKIN: &str = "mogged:skin";
pub const CH_STATE: &str = "mogged:state";
pub const CH_REMOVE: &str = "mogged:remove";

/// Horizon keeps every skin in a 64x64 atlas slot.
pub const SIZE: usize = 64;
pub const SKIN_BYTES: usize = SIZE * SIZE * 4;
/// Custom presets besides the account skin (preset 0).
pub const PRESETS: usize = 2;
/// The animation lengths the settings button cycles through, in ms.
pub const DURATIONS: [u32; 7] = [200, 500, 800, 1200, 1600, 2000, 3000];
pub const MIN_DURATION_MS: u32 = 200;
pub const MAX_DURATION_MS: u32 = 5000;
/// Half the width of the glowing band of the sweep, as in the Fabric mod.
pub const HALF_BAND: f32 = 0.06;
pub const BAND: f32 = 2.0 * HALF_BAND;
/// Messages a server sends one player per tick (Horizon allows 32).
pub const SERVER_SENDS_PER_TICK: usize = 24;

// ---------------------------------------------------------------------------
// A small JSON reader and writer, so `config/mogged.json` stays the Fabric
// mod's file: unknown keys are kept as they are and written back in order.
pub mod json {
    #[derive(Clone, Debug, PartialEq)]
    pub enum Value {
        Null,
        Bool(bool),
        Num(f64),
        Str(String),
        Arr(Vec<Value>),
        Obj(Vec<(String, Value)>),
    }

    impl Value {
        pub fn get(&self, key: &str) -> Option<&Value> {
            match self {
                Value::Obj(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
                _ => None,
            }
        }
        pub fn as_str(&self) -> Option<&str> {
            match self {
                Value::Str(s) => Some(s),
                _ => None,
            }
        }
        pub fn as_bool(&self) -> Option<bool> {
            match self {
                Value::Bool(b) => Some(*b),
                _ => None,
            }
        }
        pub fn as_f64(&self) -> Option<f64> {
            match self {
                Value::Num(n) => Some(*n),
                _ => None,
            }
        }
        /// Sets `key` of an object, keeping its place if it is there; a
        /// value that isn't an object becomes one.
        pub fn set(&mut self, key: &str, value: Value) {
            if !matches!(self, Value::Obj(_)) {
                *self = Value::Obj(Vec::new());
            }
            if let Value::Obj(entries) = self {
                if let Some(entry) = entries.iter_mut().find(|(k, _)| k == key) {
                    entry.1 = value;
                } else {
                    entries.push((key.to_owned(), value));
                }
            }
        }
        /// The object at `key`, made when missing.
        pub fn object_mut(&mut self, key: &str) -> &mut Value {
            if !matches!(self.get(key), Some(Value::Obj(_))) {
                self.set(key, Value::Obj(Vec::new()));
            }
            match self {
                Value::Obj(entries) => &mut entries.iter_mut().find(|(k, _)| k == key).unwrap().1,
                _ => unreachable!(),
            }
        }
    }

    pub fn parse(text: &str) -> Result<Value, String> {
        let mut p = Parser {
            s: text.as_bytes(),
            i: 0,
        };
        // A UTF-8 byte order mark, as some editors write.
        if p.s.starts_with(&[0xEF, 0xBB, 0xBF]) {
            p.i = 3;
        }
        let value = p.value(0)?;
        p.ws();
        if p.i != p.s.len() {
            return Err(format!("unexpected text at byte {}", p.i));
        }
        Ok(value)
    }

    struct Parser<'a> {
        s: &'a [u8],
        i: usize,
    }

    impl Parser<'_> {
        fn ws(&mut self) {
            while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\n' | b'\r') {
                self.i += 1;
            }
        }
        fn err<T>(&self, what: &str) -> Result<T, String> {
            Err(format!("{what} at byte {}", self.i))
        }
        fn value(&mut self, depth: usize) -> Result<Value, String> {
            if depth > 64 {
                return self.err("nested too deep");
            }
            self.ws();
            match self.s.get(self.i) {
                None => self.err("unexpected end"),
                Some(b'{') => {
                    self.i += 1;
                    let mut entries = Vec::new();
                    self.ws();
                    if self.s.get(self.i) == Some(&b'}') {
                        self.i += 1;
                        return Ok(Value::Obj(entries));
                    }
                    loop {
                        self.ws();
                        let key = self.string()?;
                        self.ws();
                        if self.s.get(self.i) != Some(&b':') {
                            return self.err("expected ':'");
                        }
                        self.i += 1;
                        let value = self.value(depth + 1)?;
                        entries.push((key, value));
                        self.ws();
                        match self.s.get(self.i) {
                            Some(b',') => self.i += 1,
                            Some(b'}') => {
                                self.i += 1;
                                return Ok(Value::Obj(entries));
                            }
                            _ => return self.err("expected ',' or '}'"),
                        }
                    }
                }
                Some(b'[') => {
                    self.i += 1;
                    let mut items = Vec::new();
                    self.ws();
                    if self.s.get(self.i) == Some(&b']') {
                        self.i += 1;
                        return Ok(Value::Arr(items));
                    }
                    loop {
                        items.push(self.value(depth + 1)?);
                        self.ws();
                        match self.s.get(self.i) {
                            Some(b',') => self.i += 1,
                            Some(b']') => {
                                self.i += 1;
                                return Ok(Value::Arr(items));
                            }
                            _ => return self.err("expected ',' or ']'"),
                        }
                    }
                }
                Some(b'"') => Ok(Value::Str(self.string()?)),
                Some(b't') => self.word("true", Value::Bool(true)),
                Some(b'f') => self.word("false", Value::Bool(false)),
                Some(b'n') => self.word("null", Value::Null),
                Some(_) => self.number(),
            }
        }
        fn word(&mut self, word: &str, value: Value) -> Result<Value, String> {
            if self.s[self.i..].starts_with(word.as_bytes()) {
                self.i += word.len();
                Ok(value)
            } else {
                self.err("unknown word")
            }
        }
        fn number(&mut self) -> Result<Value, String> {
            let start = self.i;
            while self.i < self.s.len() && matches!(self.s[self.i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
                self.i += 1;
            }
            let text = std::str::from_utf8(&self.s[start..self.i]).unwrap_or("");
            match text.parse::<f64>() {
                Ok(n) if !text.is_empty() => Ok(Value::Num(n)),
                _ => {
                    self.i = start;
                    self.err("bad number")
                }
            }
        }
        fn hex4(&mut self) -> Result<u32, String> {
            let Some(digits) = self.s.get(self.i..self.i + 4) else {
                return self.err("short \\u escape");
            };
            let text = std::str::from_utf8(digits).map_err(|_| "bad \\u escape".to_owned())?;
            let code = u32::from_str_radix(text, 16).map_err(|_| "bad \\u escape".to_owned())?;
            self.i += 4;
            Ok(code)
        }
        fn string(&mut self) -> Result<String, String> {
            if self.s.get(self.i) != Some(&b'"') {
                return self.err("expected a string");
            }
            self.i += 1;
            let mut out: Vec<u8> = Vec::new();
            loop {
                let Some(&c) = self.s.get(self.i) else {
                    return self.err("unterminated string");
                };
                self.i += 1;
                match c {
                    b'"' => break,
                    b'\\' => {
                        let Some(&e) = self.s.get(self.i) else {
                            return self.err("unterminated escape");
                        };
                        self.i += 1;
                        let ch = match e {
                            b'"' => '"',
                            b'\\' => '\\',
                            b'/' => '/',
                            b'b' => '\u{8}',
                            b'f' => '\u{c}',
                            b'n' => '\n',
                            b'r' => '\r',
                            b't' => '\t',
                            b'u' => {
                                let mut code = self.hex4()?;
                                if (0xD800..0xDC00).contains(&code) && self.s[self.i..].starts_with(b"\\u") {
                                    self.i += 2;
                                    let low = self.hex4()?;
                                    code = 0x10000 + ((code - 0xD800) << 10) + (low.wrapping_sub(0xDC00) & 0x3FF);
                                }
                                char::from_u32(code).unwrap_or('\u{FFFD}')
                            }
                            _ => return self.err("bad escape"),
                        };
                        let mut buf = [0u8; 4];
                        out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                    }
                    _ => out.push(c),
                }
            }
            String::from_utf8(out).map_err(|_| "a string is not UTF-8".to_owned())
        }
    }

    fn quote(s: &str, out: &mut String) {
        out.push('"');
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
    }

    /// Pretty JSON with two-space indents, as Gson's pretty printer writes it.
    pub fn write(value: &Value) -> String {
        let mut out = String::new();
        write_into(value, 0, &mut out);
        out
    }

    fn write_into(value: &Value, indent: usize, out: &mut String) {
        let pad = |n: usize, out: &mut String| out.push_str(&"  ".repeat(n));
        match value {
            Value::Null => out.push_str("null"),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::Num(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    out.push_str(&format!("{}", *n as i64));
                } else if n.is_finite() {
                    out.push_str(&format!("{n}"));
                } else {
                    out.push_str("null");
                }
            }
            Value::Str(s) => quote(s, out),
            Value::Arr(items) => {
                if items.is_empty() {
                    out.push_str("[]");
                    return;
                }
                out.push_str("[\n");
                for (i, item) in items.iter().enumerate() {
                    pad(indent + 1, out);
                    write_into(item, indent + 1, out);
                    if i + 1 < items.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                pad(indent, out);
                out.push(']');
            }
            Value::Obj(entries) => {
                if entries.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push_str("{\n");
                for (i, (k, v)) in entries.iter().enumerate() {
                    pad(indent + 1, out);
                    quote(k, out);
                    out.push_str(": ");
                    write_into(v, indent + 1, out);
                    if i + 1 < entries.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                pad(indent, out);
                out.push('}');
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Settings: the Fabric mod's `config/mogged.json`.

#[derive(Clone, Debug, PartialEq)]
pub struct Preset {
    /// A file path (absolute, or relative to `config/`), a `file:` URI or an
    /// http(s) URL; empty for none.
    pub skin: String,
    pub slim: bool,
}

#[derive(Clone, Debug)]
pub struct Config {
    /// The file as read, so keys this port doesn't use (capes, elytra) stay.
    root: json::Value,
    pub presets: [Preset; PRESETS],
    /// 0 is the account skin, 1 and 2 the presets.
    pub active: u8,
    /// Which presets the switch key cycles through: Default, 1, 2.
    pub cycle: [bool; PRESETS + 1],
    pub duration_ms: u32,
    pub effects: bool,
}

/// A button of the settings page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    /// Wear this skin now: 0 the account's, 1 and 2 the presets.
    Wear(u8),
    Duration,
    Effects,
    /// Preset n's model, slim or classic.
    Model(u8),
    /// Where preset n's skin is read from: a line of text on the page.
    Place(u8),
    OpenFolder,
}

/// What a click on the settings page asks the game to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    Nothing,
    /// A setting changed: save the file.
    Save,
    /// A preset's model changed: save and send it again.
    SaveAndResend,
    /// Wear this skin (with the sweep), then save.
    Wear(u8),
    /// Open the skins folder in the system's file manager.
    OpenFolder,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            root: json::Value::Obj(Vec::new()),
            presets: [
                Preset {
                    skin: String::new(),
                    slim: false,
                },
                Preset {
                    skin: String::new(),
                    slim: false,
                },
            ],
            active: 0,
            cycle: [true; PRESETS + 1],
            duration_ms: 1200,
            effects: true,
        }
    }
}

impl Config {
    pub fn parse(text: &str) -> Result<Self, String> {
        let root = json::parse(text)?;
        if !matches!(root, json::Value::Obj(_)) {
            return Err("the file is not a JSON object".into());
        }
        let mut config = Config::default();
        for (i, key) in ["preset1", "preset2"].iter().enumerate() {
            if let Some(p) = root.get(key) {
                config.presets[i].skin = p.get("skin").and_then(json::Value::as_str).unwrap_or("").to_owned();
                config.presets[i].slim = p.get("slim").and_then(json::Value::as_bool).unwrap_or(false);
            }
        }
        if let Some(n) = root.get("activePreset").and_then(json::Value::as_f64) {
            config.active = (n.max(0.0) as u8).min(PRESETS as u8);
        }
        if let Some(json::Value::Arr(items)) = root.get("cycle")
            && items.len() == PRESETS + 1
        {
            for (i, item) in items.iter().enumerate() {
                config.cycle[i] = item.as_bool().unwrap_or(true);
            }
        }
        if let Some(n) = root.get("durationMs").and_then(json::Value::as_f64) {
            config.duration_ms = (n.max(0.0) as u32).clamp(MIN_DURATION_MS, MAX_DURATION_MS);
        }
        if let Some(b) = root.get("effects").and_then(json::Value::as_bool) {
            config.effects = b;
        }
        config.root = root;
        Ok(config)
    }

    /// The file's text: the keys as read, with this port's values written in.
    pub fn to_text(&self) -> String {
        let mut root = self.root.clone();
        for (i, key) in ["preset1", "preset2"].iter().enumerate() {
            let p = root.object_mut(key);
            p.set("skin", json::Value::Str(self.presets[i].skin.clone()));
            p.set("slim", json::Value::Bool(self.presets[i].slim));
            // The Fabric mod's other preset keys, so it reads the file as its own.
            if p.get("cape").is_none() {
                p.set("cape", json::Value::Str(String::new()));
            }
            if p.get("separateElytra").is_none() {
                p.set("separateElytra", json::Value::Bool(false));
            }
            if p.get("elytra").is_none() {
                p.set("elytra", json::Value::Str(String::new()));
            }
        }
        root.set("activePreset", json::Value::Num(f64::from(self.active)));
        root.set("cycle", json::Value::Arr(self.cycle.iter().map(|&b| json::Value::Bool(b)).collect()));
        root.set("durationMs", json::Value::Num(f64::from(self.duration_ms)));
        root.set("effects", json::Value::Bool(self.effects));
        json::write(&root) + "\n"
    }

    /// A click on `button` of the settings page.
    pub fn click(&mut self, button: Button) -> Click {
        match button {
            Button::Wear(p) => Click::Wear(p),
            Button::Duration => {
                let next = DURATIONS.iter().position(|&d| d > self.duration_ms).unwrap_or(0);
                self.duration_ms = DURATIONS[next];
                Click::Save
            }
            Button::Effects => {
                self.effects = !self.effects;
                Click::Save
            }
            Button::Model(n) => match self.presets.get_mut(usize::from(n).wrapping_sub(1)) {
                Some(p) => {
                    p.slim = !p.slim;
                    Click::SaveAndResend
                }
                None => Click::Nothing,
            },
            Button::Place(_) | Button::OpenFolder => Click::OpenFolder,
        }
    }

    /// Where preset `n`'s skin is read from, as the settings page shows it:
    /// the default file under `config/`, or the path or URL as typed
    /// (shortened from the front when long).
    pub fn place(&self, n: u8) -> String {
        let raw = self
            .presets
            .get(usize::from(n).wrapping_sub(1))
            .map_or("", |p| p.skin.trim().trim_matches('"').trim());
        if raw.is_empty() {
            return format!("config/{ID}/preset{n}.png");
        }
        const MAX: usize = 48;
        let chars: Vec<char> = raw.chars().collect();
        if chars.len() <= MAX {
            raw.to_owned()
        } else {
            let tail: String = chars[chars.len() - (MAX - 3)..].iter().collect();
            format!("...{tail}")
        }
    }

}

// ---------------------------------------------------------------------------
// Images.

/// Where a preset's skin comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    None,
    File(std::path::PathBuf),
    Url(String),
}

/// A preset's `skin` as the player typed it: quotes around it dropped, a
/// `file:` URI turned into a path, a relative path read from `config/`.
/// Empty means `config/mogged/preset<n>.png`, if that file is there.
pub fn source(config_dir: &std::path::Path, preset: usize, raw: &str) -> Source {
    let s = raw.trim().trim_matches('"').trim();
    let lower = s.to_ascii_lowercase();
    if s.is_empty() {
        let file = config_dir.join(ID).join(format!("preset{}.png", preset + 1));
        return if file.is_file() { Source::File(file) } else { Source::None };
    }
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return Source::Url(s.to_owned());
    }
    if lower.starts_with("file:") {
        let mut path = &s[5..];
        path = path.trim_start_matches('/');
        // file:///C:/x on Windows; file:///home/x elsewhere.
        let path = if path.len() > 1 && path.as_bytes()[1] == b':' {
            path.to_owned()
        } else {
            format!("/{path}")
        };
        return Source::File(std::path::PathBuf::from(percent_decode(&path)));
    }
    let path = std::path::PathBuf::from(s);
    if path.is_absolute() {
        Source::File(path)
    } else {
        Source::File(config_dir.join(path))
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Some(byte) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A decoded skin brought to Horizon's 64 px width: square skins (64x64 and
/// HD ones, 128x128 up to 1024x1024) become 64x64, old 2:1 skins 64x32, by
/// nearest pixel. Returns the height and the RGBA.
pub fn to_64_wide(width: u32, height: u32, rgba: &[u8]) -> Result<(u32, Vec<u8>), String> {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || rgba.len() < w * h * 4 {
        return Err("the image is empty".into());
    }
    let out_h = if w == h {
        64
    } else if w == 2 * h {
        32
    } else {
        return Err(format!("a skin is 64x64 (or 64x32), not {width}x{height}"));
    };
    if w % 64 != 0 || w > 4096 {
        return Err(format!("a skin's width is a multiple of 64, not {width}"));
    }
    let mut out = vec![0u8; 64 * out_h * 4];
    for y in 0..out_h {
        for x in 0..64 {
            let (sx, sy) = (x * w / 64, y * h / out_h);
            let s = (sy * w + sx) * 4;
            let d = (y * 64 + x) * 4;
            out[d..d + 4].copy_from_slice(&rgba[s..s + 4]);
        }
    }
    Ok((out_h as u32, out))
}

// ---------------------------------------------------------------------------
// The sweep: each pixel's moment to switch, following its height on the
// model, so the change runs from head to feet (the Fabric mod's
// TransitionMap and TextureBlend).

// Skin UV boxes (64 px layout): u, v, width, height, depth, model top (px from head), model height.
const BOXES: [[i32; 7]; 12] = [
    [0, 0, 8, 8, 8, 0, 8],
    [32, 0, 8, 8, 8, 0, 8],
    [16, 16, 8, 12, 4, 8, 12],
    [16, 32, 8, 12, 4, 8, 12],
    [40, 16, 4, 12, 4, 8, 12],
    [40, 32, 4, 12, 4, 8, 12],
    [32, 48, 4, 12, 4, 8, 12],
    [48, 48, 4, 12, 4, 8, 12],
    [0, 16, 4, 12, 4, 20, 12],
    [0, 32, 4, 12, 4, 20, 12],
    [16, 48, 4, 12, 4, 20, 12],
    [0, 48, 4, 12, 4, 20, 12],
];
const MODEL_HEIGHT: f32 = 32.0;

/// Relative height on the model (0 = top of head, 1 = feet) of a UV pixel.
fn height_at(u: f32, v: f32) -> f32 {
    for b in BOXES {
        let [bu, bv, w, h, d, top, height] = b.map(|x| x as f32);
        let inside = u >= bu && u < bu + 2.0 * d + 2.0 * w && v >= bv && v < bv + d + h;
        if !inside {
            continue;
        }
        let local = if v >= bv + d {
            (v - (bv + d)) / h
        } else if u >= bu + d && u < bu + d + 2.0 * w {
            if u < bu + d + w { 0.0 } else { 1.0 }
        } else {
            continue;
        };
        return (top + local * height) / MODEL_HEIGHT;
    }
    0.5
}

/// The Fabric mod's integer hash noise, with Java's wrapping `int` maths.
fn noise(x: i32, y: i32, seed: i32) -> f32 {
    let mut hash = x
        .wrapping_mul(374761393)
        .wrapping_add(y.wrapping_mul(668265263))
        .wrapping_add(seed.wrapping_mul(1442695041));
    hash = (hash ^ ((hash as u32) >> 13) as i32).wrapping_mul(1274126177);
    hash ^= ((hash as u32) >> 16) as i32;
    (hash & 0xFFFF) as f32 / 65535.0
}

/// Each pixel's switch moment (0..1) for a 64x64 skin.
pub fn thresholds(seed: i32) -> Vec<f32> {
    let mut map = vec![0.0f32; SIZE * SIZE];
    for y in 0..SIZE as i32 {
        for x in 0..SIZE as i32 {
            let height = height_at(x as f32, y as f32);
            let n = noise(x, y, seed) * 0.6 + noise(x / 2, y / 2, seed.wrapping_mul(31).wrapping_add(7)) * 0.4;
            map[(y as usize) * SIZE + x as usize] = (0.82 * height + 0.18 * n).clamp(0.0, 1.0);
        }
    }
    map
}

/// Tints a pixel towards cyan-white; transparent pixels in the band light up too.
fn glow(base: &[u8], a: &[u8], b: &[u8], strength: f32) -> [u8; 4] {
    let alpha = (base[3] as i32).max((a[3].max(b[3]) as f32 * strength) as i32);
    let mix = 0.25 + 0.75 * strength;
    let (r, g, bl) = (base[0] as f32, base[1] as f32, base[2] as f32);
    let r = r + (170.0 - r) * mix * 0.9 + 60.0 * strength;
    let g = g + (245.0 - g) * mix;
    let bl = bl + (255.0 - bl) * mix;
    let c = |v: f32| v.clamp(0.0, 255.0) as u8;
    [c(r), c(g), c(bl), alpha.clamp(0, 255) as u8]
}

/// One frame of the sweep at `line`: pixels well below it show `to`, well
/// above it `from`, and those within `HALF_BAND` of it glow.
pub fn blend(from: &[u8], to: &[u8], thresholds: &[f32], line: f32) -> Vec<u8> {
    let mut out = vec![0u8; SKIN_BYTES];
    for (i, &t) in thresholds.iter().enumerate().take(SIZE * SIZE) {
        let p = i * 4;
        let (f, n) = (&from[p..p + 4], &to[p..p + 4]);
        let delta = line - t;
        let px: [u8; 4] = if delta < -HALF_BAND {
            [f[0], f[1], f[2], f[3]]
        } else if delta > HALF_BAND {
            [n[0], n[1], n[2], n[3]]
        } else {
            let k = (delta + HALF_BAND) / BAND;
            let strength = 1.0 - (k - 0.5).abs() * 2.0;
            glow(if k < 0.5 { f } else { n }, f, n, strength)
        };
        out[p..p + 4].copy_from_slice(&px);
    }
    out
}

/// The sweep's line for animation progress `t` (0..1): eased in and out, and
/// widened so the band fully enters and leaves the skin.
pub fn line_at(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let eased = if t < 0.5 {
        2.0 * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
    };
    eased * (1.0 + BAND) - HALF_BAND
}

// ---------------------------------------------------------------------------
// Messages. Client to server: the payload; server to clients: the sender's
// UUID (16 bytes, big-endian) then the payload.
//
// mogged:skin   preset (1 or 2), slim (0/1), then 64x64 RGBA or nothing (cleared)
// mogged:state  active preset (0..2), animate (0/1), duration ms (u16 LE), effects (0/1)
// mogged:remove (server only) the UUID alone: that player left

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct State {
    pub active: u8,
    pub animate: bool,
    pub duration_ms: u16,
    pub effects: bool,
}

pub fn encode_skin(preset: u8, slim: bool, rgba: Option<&[u8]>) -> Vec<u8> {
    let mut out = vec![preset, u8::from(slim)];
    if let Some(rgba) = rgba {
        out.extend_from_slice(rgba);
    }
    out
}

/// (preset, slim, RGBA or None when cleared), or None for a bad message.
pub fn decode_skin(data: &[u8]) -> Option<(u8, bool, Option<Vec<u8>>)> {
    let (&preset, rest) = data.split_first()?;
    let (&slim, rgba) = rest.split_first()?;
    if !(1..=PRESETS as u8).contains(&preset) || slim > 1 {
        return None;
    }
    match rgba.len() {
        0 => Some((preset, slim == 1, None)),
        SKIN_BYTES => Some((preset, slim == 1, Some(rgba.to_vec()))),
        _ => None,
    }
}

pub fn encode_state(s: State) -> Vec<u8> {
    let d = s.duration_ms.to_le_bytes();
    vec![s.active, u8::from(s.animate), d[0], d[1], u8::from(s.effects)]
}

pub fn decode_state(data: &[u8]) -> Option<State> {
    let [active, animate, d0, d1, effects] = data else {
        return None;
    };
    if *active > PRESETS as u8 || *animate > 1 || *effects > 1 {
        return None;
    }
    Some(State {
        active: *active,
        animate: *animate == 1,
        duration_ms: u16::from_le_bytes([*d0, *d1]).min(MAX_DURATION_MS as u16),
        effects: *effects == 1,
    })
}

pub fn with_player(player: u128, data: &[u8]) -> Vec<u8> {
    let mut out = player.to_be_bytes().to_vec();
    out.extend_from_slice(data);
    out
}

pub fn split_player(data: &[u8]) -> Option<(u128, &[u8])> {
    if data.len() < 16 {
        return None;
    }
    let (id, rest) = data.split_at(16);
    Some((u128::from_be_bytes(id.try_into().ok()?), rest))
}

// ---------------------------------------------------------------------------
// The client: the window's own presets and what it knows of other players.

#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub slim: bool,
    pub rgba: Vec<u8>,
    /// Bumped on each change, so the atlas gets a fresh location.
    pub generation: u32,
}

#[derive(Debug)]
pub struct Anim {
    pub from: u8,
    pub to: u8,
    pub start: Instant,
    pub duration: Duration,
    /// The account skin's pixels, when one side of the sweep is it: asked of
    /// the game once (`default_asked`); without them the band sweeps over the
    /// other side's skin alone.
    pub default_rgba: Option<Vec<u8>>,
    pub default_asked: bool,
    pub thresholds: Vec<f32>,
    /// A frame is in the atlas.
    pub drawn: bool,
    pub effects: bool,
    pub effects_played: bool,
}

#[derive(Debug, Default)]
pub struct Skins {
    pub presets: [Option<Look>; PRESETS],
    pub active: u8,
    pub anim: Option<Anim>,
}

impl Skins {
    pub fn look(&self, preset: u8) -> Option<&Look> {
        if preset == 0 {
            return None;
        }
        self.presets.get(preset as usize - 1)?.as_ref()
    }
    pub fn has_content(&self, preset: u8) -> bool {
        preset == 0 || self.look(preset).is_some()
    }
    fn set(&mut self, preset: u8, look: Option<(bool, Vec<u8>)>) {
        let Some(slot) = self.presets.get_mut(preset as usize - 1) else {
            return;
        };
        let generation = slot.as_ref().map_or(0, |l| l.generation.wrapping_add(1));
        *slot = look.map(|(slim, rgba)| Look { slim, rgba, generation });
    }
    /// Shows `to`, sweeping from what is shown now when `duration` isn't 0.
    fn switch(&mut self, to: u8, duration: Duration, effects: bool, now: Instant, seed: i32) {
        let from = self.active;
        self.active = to;
        if from == to || duration.is_zero() || (from == 0 && to == 0) {
            self.anim = None;
            return;
        }
        self.anim = Some(Anim {
            from,
            to,
            start: now,
            duration,
            default_rgba: None,
            default_asked: !(from == 0 || to == 0),
            thresholds: thresholds(seed),
            drawn: false,
            effects,
            effects_played: false,
        });
    }
}

/// What the game draws for a player.
#[derive(Clone, Debug, PartialEq)]
pub enum Display {
    /// Their own skin, as Horizon resolved it.
    Vanilla,
    /// A texture of this mod in the atlas, at `location`.
    Texture { location: String, slim: bool },
}

pub fn preset_location(player: u128, preset: u8, generation: u32) -> String {
    format!("mogged/{player:032x}/p{preset}/{generation}")
}

pub fn blend_location(player: u128) -> String {
    format!("mogged/{player:032x}/blend")
}

#[derive(Debug, Default)]
pub struct Client {
    /// The settings and presets are read (then watched for changes).
    pub loaded: bool,
    pub config: Config,
    /// This window's player, as the skin code names it.
    pub own: Option<u128>,
    pub me: Skins,
    pub others: HashMap<u128, Skins>,
    /// Each player's own skin as Horizon resolved it: location and slim.
    pub vanilla: HashMap<u128, (String, bool)>,
    /// The server has our presets and state.
    pub synced: bool,
    /// Ticks until the next try to send them.
    pub retry: u32,
    /// The active preset changed since it was sent.
    pub state_dirty: bool,
    seed: i32,
    /// Errors to show in chat once, from loading the presets.
    pub messages: Vec<String>,
}

impl Client {
    fn next_seed(&mut self) -> i32 {
        self.seed = self.seed.wrapping_mul(1103515245).wrapping_add(12345);
        self.seed
    }

    pub fn is_own(&self, player: u128) -> bool {
        self.own == Some(player)
    }

    pub fn skins(&self, player: u128) -> Option<&Skins> {
        if self.is_own(player) {
            Some(&self.me)
        } else {
            self.others.get(&player)
        }
    }

    fn skins_mut(&mut self, player: u128) -> &mut Skins {
        if self.is_own(player) {
            &mut self.me
        } else {
            self.others.entry(player).or_default()
        }
    }

    /// Our own preset `preset` (1 or 2) was loaded, or failed (None).
    pub fn set_own_preset(&mut self, preset: u8, look: Option<(bool, Vec<u8>)>) {
        self.me.set(preset, look);
        if self.me.active == preset && self.me.look(preset).is_none() {
            self.me.active = 0;
            self.me.anim = None;
        }
        self.synced = false;
        self.retry = 0;
    }

    /// The model (slim or classic) of our own preset changed.
    pub fn set_own_slim(&mut self, preset: u8, slim: bool) {
        if let Some(Some(look)) = self.me.presets.get_mut(preset as usize - 1) {
            look.slim = slim;
            look.generation = look.generation.wrapping_add(1);
        }
        self.synced = false;
        self.retry = 0;
    }

    /// The settings were read: the preset to start with.
    pub fn restore_active(&mut self) {
        let active = self.config.active;
        self.me.active = if self.me.has_content(active) { active } else { 0 };
        self.me.anim = None;
    }

    /// The settings page: each button and its label. A preset is offered
    /// (to wear, and its model) only while it has a skin.
    pub fn page(&self) -> Vec<(Button, String)> {
        let on = |b: bool| if b { "ON" } else { "OFF" };
        let mark = |p: u8| if self.me.active == p { "[x]" } else { "[ ]" };
        let mut out = vec![(Button::Wear(0), format!("{} Account skin", mark(0)))];
        for n in 1..=PRESETS as u8 {
            if self.me.has_content(n) {
                out.push((Button::Wear(n), format!("{} Preset {n}", mark(n))));
            }
        }
        for n in 1..=PRESETS as u8 {
            if self.me.has_content(n) {
                let slim = self.config.presets[usize::from(n) - 1].slim;
                out.push((Button::Model(n), format!("Preset {n} model: {}", if slim { "Slim" } else { "Classic" })));
            }
        }
        out.push((Button::Duration, format!("Animation: {:.1} s", self.config.duration_ms as f32 / 1000.0)));
        out.push((Button::Effects, format!("Effects: {}", on(self.config.effects))));
        for n in 1..=PRESETS as u8 {
            let state = if self.me.has_content(n) { "" } else { " (none)" };
            out.push((Button::Place(n), format!("Preset {n}: {}{state}", self.config.place(n))));
        }
        out.push((Button::OpenFolder, "Open skins folder".to_owned()));
        out
    }

    /// The switch key: the next preset that is in the cycle and has a skin.
    /// None when there is nowhere else to go.
    pub fn toggle(&mut self, now: Instant) -> Option<u8> {
        let current = self.me.active;
        let count = PRESETS as u8 + 1;
        let next = (1..count)
            .map(|step| (current + step) % count)
            .find(|&p| self.config.cycle[p as usize] && self.me.has_content(p))?;
        self.select(next, now);
        Some(next)
    }

    /// Shows our preset `preset`, with the sweep.
    pub fn select(&mut self, preset: u8, now: Instant) {
        let duration = Duration::from_millis(u64::from(self.config.duration_ms));
        let effects = self.config.effects;
        let seed = self.next_seed();
        self.me.switch(preset, duration, effects, now, seed);
        self.config.active = preset;
        self.state_dirty = true;
    }

    pub fn own_state(&self, animate: bool) -> State {
        State {
            active: self.me.active,
            animate,
            duration_ms: self.config.duration_ms.min(u32::from(u16::MAX)) as u16,
            effects: self.config.effects,
        }
    }

    /// The messages that give the server our presets and state, in order.
    pub fn sync_messages(&self) -> Vec<(&'static str, Vec<u8>)> {
        let mut out = Vec::new();
        for preset in 1..=PRESETS as u8 {
            let look = self.me.look(preset);
            out.push((
                CH_SKIN,
                encode_skin(preset, look.is_some_and(|l| l.slim), look.map(|l| l.rgba.as_slice())),
            ));
        }
        out.push((CH_STATE, encode_state(self.own_state(false))));
        out
    }

    /// A message from the server.
    pub fn from_server(&mut self, channel: &str, data: &[u8], now: Instant) {
        let Some((player, rest)) = split_player(data) else {
            return;
        };
        // Our own state is ours: the server only relays it.
        if self.is_own(player) {
            return;
        }
        match channel {
            CH_SKIN => {
                if let Some((preset, slim, rgba)) = decode_skin(rest) {
                    let skins = self.skins_mut(player);
                    skins.set(preset, rgba.map(|rgba| (slim, rgba)));
                    if skins.active == preset && skins.look(preset).is_none() {
                        skins.active = 0;
                        skins.anim = None;
                    }
                }
            }
            CH_STATE => {
                if let Some(state) = decode_state(rest) {
                    let seed = self.next_seed();
                    let skins = self.skins_mut(player);
                    let to = if skins.has_content(state.active) { state.active } else { 0 };
                    let duration = if state.animate {
                        Duration::from_millis(u64::from(state.duration_ms))
                    } else {
                        Duration::ZERO
                    };
                    skins.switch(to, duration, state.effects, now, seed);
                }
            }
            CH_REMOVE => {
                self.others.remove(&player);
            }
            _ => {}
        }
    }

    /// Left the world: other players' presets go, ours are sent again on
    /// the next join.
    pub fn left_world(&mut self) {
        self.others.clear();
        self.vanilla.clear();
        self.me.anim = None;
        self.synced = false;
        self.retry = 0;
    }

    /// What to draw for `player`, whose own skin is `vanilla`; records it.
    pub fn display(&mut self, player: u128, vanilla: &(String, bool), now: Instant) -> Display {
        // Our own textures never stand for a player's own skin.
        if !vanilla.0.starts_with("mogged/") {
            self.vanilla.insert(player, vanilla.clone());
        }
        let Some(skins) = self.skins(player) else {
            return Display::Vanilla;
        };
        let vanilla_slim = vanilla.1;
        let slim_of = |preset: u8| skins.look(preset).map_or(vanilla_slim, |l| l.slim);
        if let Some(anim) = &skins.anim
            && now.duration_since(anim.start) < anim.duration
        {
            let t = now.duration_since(anim.start).as_secs_f32() / anim.duration.as_secs_f32();
            let side = if t >= 0.5 { anim.to } else { anim.from };
            if anim.drawn {
                return Display::Texture {
                    location: blend_location(player),
                    slim: slim_of(side),
                };
            }
            return match skins.look(anim.from) {
                Some(l) => Display::Texture {
                    location: preset_location(player, anim.from, l.generation),
                    slim: l.slim,
                },
                None => Display::Vanilla,
            };
        }
        match skins.look(skins.active) {
            Some(l) => Display::Texture {
                location: preset_location(player, skins.active, l.generation),
                slim: l.slim,
            },
            None => Display::Vanilla,
        }
    }

    /// The pixels of the texture at `location`, if it is one of ours that
    /// isn't animated (the game stores it into the atlas when it's missing).
    pub fn pixels(&self, location: &str) -> Option<&[u8]> {
        let rest = location.strip_prefix("mogged/")?;
        let (id, rest) = rest.split_once('/')?;
        let player = u128::from_str_radix(id, 16).ok()?;
        let preset: u8 = rest.strip_prefix('p')?.split('/').next()?.parse().ok()?;
        self.skins(player)?.look(preset).map(|l| l.rgba.as_slice())
    }

    /// Players whose sweep has the account skin on one side and hasn't asked
    /// for its pixels yet, with that skin's location.
    pub fn defaults_wanted(&self) -> Vec<(u128, String)> {
        let all = self.own.map(|o| (o, &self.me)).into_iter().chain(self.others.iter().map(|(k, v)| (*k, v)));
        all.filter(|(_, s)| s.anim.as_ref().is_some_and(|a| !a.default_asked))
            .filter_map(|(p, _)| self.vanilla.get(&p).map(|v| (p, v.0.clone())))
            .collect()
    }

    /// The account skin's pixels for `player`'s sweep (None: not to be had).
    pub fn set_default_pixels(&mut self, player: u128, rgba: Option<Vec<u8>>) {
        if let Some(anim) = self.skins_mut(player).anim.as_mut() {
            anim.default_asked = true;
            anim.default_rgba = rgba.filter(|r| r.len() == SKIN_BYTES);
        }
    }

    /// This frame's sweeps: each animated player's blended texture and its
    /// location, and the players whose sweep just started with effects on.
    /// Finished sweeps end here.
    pub fn frames(&mut self, now: Instant) -> (Vec<(String, Vec<u8>)>, Vec<u128>) {
        let mut out = Vec::new();
        let mut started = Vec::new();
        let own = self.own;
        let players: Vec<u128> = own.into_iter().chain(self.others.keys().copied()).collect();
        for player in players {
            let skins = if Some(player) == own {
                &mut self.me
            } else {
                match self.others.get_mut(&player) {
                    Some(s) => s,
                    None => continue,
                }
            };
            let Some(anim) = skins.anim.as_ref() else {
                continue;
            };
            let elapsed = now.duration_since(anim.start);
            if elapsed >= anim.duration {
                skins.anim = None;
                continue;
            }
            // The account skin's pixels are asked for first: no frame till then.
            if !anim.default_asked {
                continue;
            }
            let side = |p: u8| -> Option<&[u8]> {
                if p == 0 {
                    anim.default_rgba.as_deref()
                } else {
                    skins.look(p).map(|l| l.rgba.as_slice())
                }
            };
            let (from, to) = match (side(anim.from), side(anim.to)) {
                (Some(f), Some(t)) => (f, t),
                (Some(f), None) => (f, f),
                (None, Some(t)) => (t, t),
                (None, None) => {
                    skins.anim = None;
                    continue;
                }
            };
            let t = elapsed.as_secs_f32() / anim.duration.as_secs_f32();
            let frame = blend(from, to, &anim.thresholds, line_at(t));
            out.push((blend_location(player), frame));
            if let Some(anim) = skins.anim.as_mut() {
                anim.drawn = true;
                if anim.effects && !anim.effects_played {
                    anim.effects_played = true;
                    started.push(player);
                }
            }
        }
        (out, started)
    }
}

// ---------------------------------------------------------------------------
// The server: relays presets and state between the players who have the mod
// (the Fabric mod's SkinServer).

#[derive(Debug, Default)]
struct Stored {
    skins: [Option<(bool, Vec<u8>)>; PRESETS],
    state: Option<State>,
}

#[derive(Debug, Default)]
pub struct Server {
    players: HashMap<u128, Stored>,
    online: Vec<u128>,
    outbox: VecDeque<(u128, &'static str, Vec<u8>)>,
}

impl Server {
    fn broadcast(&mut self, from: u128, channel: &'static str, payload: &[u8]) {
        let message = with_player(from, payload);
        for &to in &self.online {
            if to != from {
                self.outbox.push_back((to, channel, message.clone()));
            }
        }
    }

    /// A message from `player`'s client.
    pub fn from_client(&mut self, player: u128, channel: &str, data: &[u8]) {
        match channel {
            CH_SKIN => {
                let Some((preset, slim, rgba)) = decode_skin(data) else {
                    return;
                };
                let stored = self.players.entry(player).or_default();
                stored.skins[preset as usize - 1] = rgba.map(|r| (slim, r));
                self.broadcast(player, CH_SKIN, data);
            }
            CH_STATE => {
                let Some(state) = decode_state(data) else {
                    return;
                };
                self.players.entry(player).or_default().state = Some(state);
                self.broadcast(player, CH_STATE, data);
            }
            _ => {}
        }
    }

    /// `player` joined: they get everyone else's presets and state.
    pub fn logged_in(&mut self, player: u128) {
        if !self.online.contains(&player) {
            self.online.push(player);
        }
        let mut snapshot = Vec::new();
        for (&other, stored) in &self.players {
            if other == player {
                continue;
            }
            for (i, skin) in stored.skins.iter().enumerate() {
                if let Some((slim, rgba)) = skin {
                    let payload = encode_skin(i as u8 + 1, *slim, Some(rgba));
                    snapshot.push((CH_SKIN, with_player(other, &payload)));
                }
            }
            if let Some(state) = stored.state {
                let state = State { animate: false, ..state };
                snapshot.push((CH_STATE, with_player(other, &encode_state(state))));
            }
        }
        for (channel, message) in snapshot {
            self.outbox.push_back((player, channel, message));
        }
    }

    /// `player` left: the others forget their presets.
    pub fn logged_out(&mut self, player: u128) {
        self.online.retain(|&p| p != player);
        self.outbox.retain(|(to, _, _)| *to != player);
        if self.players.remove(&player).is_some() {
            self.broadcast(player, CH_REMOVE, &[]);
        }
    }

    /// This tick's messages, at most `per_player` to each player; the rest
    /// wait for the next tick, in order.
    pub fn drain(&mut self, per_player: usize) -> Vec<(u128, &'static str, Vec<u8>)> {
        let mut counts: HashMap<u128, usize> = HashMap::new();
        let mut out = Vec::new();
        let mut kept = VecDeque::new();
        while let Some(item) = self.outbox.pop_front() {
            let count = counts.entry(item.0).or_default();
            if *count < per_player {
                *count += 1;
                out.push(item);
            } else {
                kept.push_back(item);
            }
        }
        self.outbox = kept;
        out
    }

    #[cfg(test)]
    pub fn pending(&self) -> usize {
        self.outbox.len()
    }
}

thread_local! {
    // The window's thread has the client; the server's thread (single
    // player's integrated server, a LAN host's, a dedicated one) the server.
    static CLIENT: RefCell<Client> = RefCell::new(Client::default());
    static SERVER: RefCell<Server> = RefCell::new(Server::default());
}

pub fn with_client<R>(f: impl FnOnce(&mut Client) -> R) -> R {
    CLIENT.with(|c| f(&mut c.borrow_mut()))
}

pub fn with_server<R>(f: impl FnOnce(&mut Server) -> R) -> R {
    SERVER.with(|s| f(&mut s.borrow_mut()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skin(fill: u8) -> Vec<u8> {
        vec![fill; SKIN_BYTES]
    }

    #[test]
    fn reads_and_writes_the_fabric_config() {
        let text = r#"{
  "preset1": {
    "skin": "C:\\Users\\me\\skins\\one.png",
    "slim": true,
    "cape": "cape.png",
    "separateElytra": false,
    "elytra": ""
  },
  "preset2": {
    "skin": "https://example.com/two.png",
    "slim": false,
    "cape": "",
    "separateElytra": true,
    "elytra": "e.png"
  },
  "activePreset": 2,
  "cycle": [
    true,
    false,
    true
  ],
  "durationMs": 900,
  "effects": false,
  "futureKey": {"a": [1, 2.5, null, "x\u00e9"]}
}"#;
        let mut config = Config::parse(text).unwrap();
        assert_eq!(config.presets[0].skin, "C:\\Users\\me\\skins\\one.png");
        assert!(config.presets[0].slim);
        assert_eq!(config.active, 2);
        assert_eq!(config.cycle, [true, false, true]);
        assert_eq!(config.duration_ms, 900);
        assert!(!config.effects);
        config.active = 1;
        config.effects = true;
        let written = config.to_text();
        let again = Config::parse(&written).unwrap();
        assert_eq!(again.active, 1);
        assert!(again.effects);
        assert_eq!(again.presets, config.presets);
        // Keys this port doesn't use are kept as they were.
        assert!(written.contains("\"cape\": \"cape.png\""));
        assert!(written.contains("\"elytra\": \"e.png\""));
        assert!(written.contains("\"futureKey\""));
        assert!(written.contains("xé"));
        assert!(written.contains("\"durationMs\": 900"));
    }

    #[test]
    fn a_missing_or_broken_config_gives_defaults_or_an_error() {
        let config = Config::parse("{}").unwrap();
        assert_eq!(config.duration_ms, 1200);
        assert!(config.effects);
        assert_eq!(config.cycle, [true; 3]);
        assert!(Config::parse("{\"preset1\": ").is_err());
        assert!(Config::parse("[1]").is_err());
        let text = Config::default().to_text();
        assert!(Config::parse(&text).is_ok());
        assert!(text.contains("\"separateElytra\": false"));
    }

    #[test]
    fn settings_buttons_cycle() {
        let mut config = Config::default();
        assert_eq!(config.click(Button::Duration), Click::Save);
        assert_eq!(config.duration_ms, 1600);
        for _ in 0..3 {
            config.click(Button::Duration);
        }
        assert_eq!(config.duration_ms, 200);
        assert_eq!(config.click(Button::Model(1)), Click::SaveAndResend);
        assert!(config.presets[0].slim);
        assert_eq!(config.click(Button::Model(3)), Click::Nothing);
        assert_eq!(config.click(Button::Place(2)), Click::OpenFolder);
        assert_eq!(config.click(Button::Wear(2)), Click::Wear(2));
        assert_eq!(config.place(1), "config/mogged/preset1.png");
        config.presets[1].skin = format!("\"C:/skins/{}.png\"", "x".repeat(60));
        assert!(config.place(2).starts_with("...") && config.place(2).ends_with("x.png"));
        assert_eq!(config.place(2).chars().count(), 48);
    }

    #[test]
    fn settings_page_offers_presets_with_a_skin() {
        let mut client = Client::default();
        let labels = |c: &Client| c.page().into_iter().map(|(_, l)| l).collect::<Vec<_>>();
        let page = labels(&client);
        assert_eq!(page[0], "[x] Account skin");
        assert!(!page.iter().any(|l| l.contains("[ ] Preset") || l.contains("model")));
        assert!(page.contains(&"Preset 1: config/mogged/preset1.png (none)".to_owned()));
        client.set_own_preset(2, Some((true, vec![0; SKIN_BYTES])));
        client.me.active = 2;
        let page = labels(&client);
        assert_eq!(page[0], "[ ] Account skin");
        assert_eq!(page[1], "[x] Preset 2");
        assert!(page.contains(&"Preset 2 model: Classic".to_owned()));
        assert!(page.contains(&"Preset 2: config/mogged/preset2.png".to_owned()));
        assert!(!page.iter().any(|l| l.ends_with("Preset 1")));
    }

    #[test]
    fn sources() {
        let dir = std::path::Path::new("/game/config");
        assert_eq!(source(dir, 0, "  https://a.b/c.png "), Source::Url("https://a.b/c.png".into()));
        assert_eq!(source(dir, 0, "\"/abs/x.png\""), Source::File("/abs/x.png".into()));
        assert_eq!(source(dir, 0, "rel/x.png"), Source::File("/game/config/rel/x.png".into()));
        assert_eq!(source(dir, 0, "file:///home/me/a%20b.png"), Source::File("/home/me/a b.png".into()));
        assert_eq!(source(dir, 0, "file:///C:/Users/a.png"), Source::File("C:/Users/a.png".into()));
        assert_eq!(source(dir, 1, ""), Source::None);
    }

    #[test]
    fn hd_and_legacy_skins_become_64_wide() {
        let mut hd = vec![0u8; 128 * 128 * 4];
        // The top-left 2x2 block of the HD skin is one pixel of the result.
        for y in 0..2 {
            for x in 0..2 {
                hd[(y * 128 + x) * 4..(y * 128 + x) * 4 + 4].copy_from_slice(&[9, 8, 7, 255]);
            }
        }
        let (h, rgba) = to_64_wide(128, 128, &hd).unwrap();
        assert_eq!(h, 64);
        assert_eq!(rgba.len(), SKIN_BYTES);
        assert_eq!(&rgba[0..4], &[9, 8, 7, 255]);
        assert_eq!(&rgba[4..8], &[0, 0, 0, 0]);
        let (h, rgba) = to_64_wide(64, 32, &vec![1; 64 * 32 * 4]).unwrap();
        assert_eq!((h, rgba.len()), (32, 64 * 32 * 4));
        assert!(to_64_wide(100, 50, &vec![0; 100 * 50 * 4]).is_err());
        assert!(to_64_wide(64, 48, &vec![0; 64 * 48 * 4]).is_err());
        assert!(to_64_wide(64, 64, &[0; 10]).is_err());
    }

    #[test]
    fn the_sweep_runs_from_head_to_feet() {
        let map = thresholds(42);
        assert_eq!(map.len(), SIZE * SIZE);
        assert!(map.iter().all(|t| (0.0..=1.0).contains(t)));
        // A head pixel (the face, 8..16 x 8..16) switches before a leg pixel.
        let head = map[10 * SIZE + 10];
        let leg = map[30 * SIZE + 5];
        assert!(head < leg, "{head} {leg}");
        // The same seed gives the same map; the Java noise wraps like int.
        assert_eq!(thresholds(42), map);
        assert!(noise(i32::MAX, i32::MIN, -7) >= 0.0);
    }

    #[test]
    fn blending_shows_from_then_to() {
        let map = thresholds(1);
        let (a, b) = (skin(10), skin(200));
        assert_eq!(blend(&a, &b, &map, line_at(0.0)), a);
        assert_eq!(blend(&a, &b, &map, line_at(1.0)), b);
        let mid = blend(&a, &b, &map, line_at(0.5));
        assert!(mid.chunks(4).any(|p| p == [10, 10, 10, 10]));
        assert!(mid.chunks(4).any(|p| p == [200, 200, 200, 200]));
        assert!(line_at(0.0) < 0.0 && line_at(1.0) > 1.0);
    }

    #[test]
    fn messages_round_trip_and_bad_ones_are_refused() {
        let rgba = skin(3);
        let m = encode_skin(2, true, Some(&rgba));
        assert_eq!(decode_skin(&m), Some((2, true, Some(rgba.clone()))));
        assert_eq!(decode_skin(&encode_skin(1, false, None)), Some((1, false, None)));
        assert_eq!(decode_skin(&[3, 0]), None);
        assert_eq!(decode_skin(&[1, 0, 5]), None);
        assert_eq!(decode_skin(&[]), None);
        let s = State {
            active: 1,
            animate: true,
            duration_ms: 1200,
            effects: false,
        };
        assert_eq!(decode_state(&encode_state(s)), Some(s));
        assert_eq!(decode_state(&[3, 0, 0, 0, 0]), None);
        assert_eq!(decode_state(&[1, 0, 0]), None);
        let id = 0x0123_4567_89ab_cdef_0011_2233_4455_6677u128;
        let framed = with_player(id, &[1, 2]);
        assert_eq!(split_player(&framed), Some((id, &[1u8, 2][..])));
        assert_eq!(split_player(&[0; 5]), None);
        // The largest message fits Horizon's 30 000 bytes.
        assert!(with_player(id, &m).len() <= 30_000);
    }

    #[test]
    fn the_key_cycles_presets_with_skins() {
        let now = Instant::now();
        let mut c = Client {
            own: Some(1),
            ..Client::default()
        };
        assert_eq!(c.toggle(now), None, "only the account skin");
        c.set_own_preset(2, Some((false, skin(5))));
        assert_eq!(c.toggle(now), Some(2));
        assert!(c.state_dirty);
        assert_eq!(c.toggle(now), Some(0));
        c.set_own_preset(1, Some((true, skin(6))));
        assert_eq!(c.toggle(now), Some(1));
        assert_eq!(c.toggle(now), Some(2));
        c.config.cycle = [true, false, true];
        assert_eq!(c.toggle(now), Some(0));
        assert_eq!(c.toggle(now), Some(2));
        // Losing the shown preset falls back to the account skin.
        c.set_own_preset(2, None);
        assert_eq!(c.me.active, 0);
    }

    #[test]
    fn display_follows_the_preset_and_the_sweep() {
        let t0 = Instant::now();
        let mut c = Client {
            own: Some(7),
            ..Client::default()
        };
        c.config.duration_ms = 1000;
        let vanilla = ("skins/abc".to_owned(), false);
        assert_eq!(c.display(7, &vanilla, t0), Display::Vanilla);
        c.set_own_preset(1, Some((true, skin(9))));
        c.select(1, t0);
        // Before the first frame: the account skin still.
        assert_eq!(c.display(7, &vanilla, t0), Display::Vanilla);
        assert_eq!(c.defaults_wanted(), vec![(7, "skins/abc".to_owned())]);
        let (frames, _) = c.frames(t0);
        assert!(frames.is_empty(), "waits for the account skin's pixels");
        c.set_default_pixels(7, Some(skin(1)));
        let (frames, started) = c.frames(t0 + Duration::from_millis(100));
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].0, blend_location(7));
        assert_eq!(started, vec![7]);
        let early = c.display(7, &vanilla, t0 + Duration::from_millis(100));
        assert_eq!(early, Display::Texture { location: blend_location(7), slim: false });
        let late = c.display(7, &vanilla, t0 + Duration::from_millis(600));
        assert_eq!(late, Display::Texture { location: blend_location(7), slim: true });
        let (_, started) = c.frames(t0 + Duration::from_millis(200));
        assert!(started.is_empty(), "effects play once");
        let (frames, _) = c.frames(t0 + Duration::from_millis(1000));
        assert!(frames.is_empty());
        let done = c.display(7, &vanilla, t0 + Duration::from_millis(1000));
        let location = preset_location(7, 1, 0);
        assert_eq!(done, Display::Texture { location: location.clone(), slim: true });
        assert_eq!(c.pixels(&location), Some(&skin(9)[..]));
        assert_eq!(c.pixels("skins/abc"), None);
        // A mod texture is never taken for the account skin.
        c.display(7, &(location, true), t0);
        assert_eq!(c.vanilla.get(&7), Some(&vanilla));
    }

    #[test]
    fn a_sweep_without_the_account_skins_pixels_still_runs() {
        let t0 = Instant::now();
        let mut c = Client {
            own: Some(7),
            ..Client::default()
        };
        c.set_own_preset(1, Some((false, skin(9))));
        c.select(1, t0);
        c.set_default_pixels(7, None);
        let (frames, _) = c.frames(t0 + Duration::from_millis(10));
        assert_eq!(frames.len(), 1);
    }

    #[test]
    fn other_players_from_the_server() {
        let t0 = Instant::now();
        let mut c = Client {
            own: Some(1),
            ..Client::default()
        };
        let other = 2u128;
        c.from_server(CH_SKIN, &with_player(other, &encode_skin(1, false, Some(&skin(4)))), t0);
        let state = State {
            active: 1,
            animate: false,
            duration_ms: 500,
            effects: true,
        };
        c.from_server(CH_STATE, &with_player(other, &encode_state(state)), t0);
        let vanilla = ("entity/player/wide/steve".to_owned(), false);
        assert_eq!(
            c.display(other, &vanilla, t0),
            Display::Texture {
                location: preset_location(other, 1, 0),
                slim: false
            }
        );
        // Our own state relayed back is ignored.
        c.from_server(CH_STATE, &with_player(1, &encode_state(state)), t0);
        assert_eq!(c.me.active, 0);
        // A state for a preset they haven't sent shows their own skin.
        let two = State { active: 2, ..state };
        c.from_server(CH_STATE, &with_player(other, &encode_state(two)), t0);
        assert_eq!(c.display(other, &vanilla, t0), Display::Vanilla);
        c.from_server(CH_REMOVE, &with_player(other, &[]), t0);
        assert!(c.others.is_empty());
        c.from_server(CH_STATE, &[1, 2, 3], t0);
        c.left_world();
        assert!(!c.synced);
    }

    #[test]
    fn sync_sends_both_presets_then_the_state() {
        let mut c = Client::default();
        c.set_own_preset(2, Some((true, skin(8))));
        let m = c.sync_messages();
        assert_eq!(m.len(), 3);
        assert_eq!(decode_skin(&m[0].1), Some((1, false, None)));
        assert_eq!(decode_skin(&m[1].1), Some((2, true, Some(skin(8)))));
        assert_eq!(m[2].0, CH_STATE);
        assert!(!decode_state(&m[2].1).unwrap().animate);
    }

    #[test]
    fn the_server_relays_and_catches_up_new_players() {
        let mut s = Server::default();
        let (a, b, c) = (1u128, 2u128, 3u128);
        s.logged_in(a);
        s.logged_in(b);
        s.from_client(a, CH_SKIN, &encode_skin(1, true, Some(&skin(1))));
        s.from_client(a, CH_STATE, &encode_state(State {
            active: 1,
            animate: true,
            duration_ms: 800,
            effects: true,
        }));
        s.from_client(a, CH_SKIN, &[9, 9]);
        let sent = s.drain(24);
        assert_eq!(sent.len(), 2);
        assert!(sent.iter().all(|(to, _, m)| *to == b && split_player(m).unwrap().0 == a));
        // A new player gets what is known, without the animation.
        s.logged_in(c);
        let sent = s.drain(24);
        assert_eq!(sent.len(), 2);
        assert!(sent.iter().all(|(to, _, _)| *to == c));
        let state = decode_state(split_player(&sent[1].2).unwrap().1).unwrap();
        assert!(!state.animate);
        // Leaving tells the others.
        s.logged_out(a);
        let sent = s.drain(24);
        assert_eq!(sent.len(), 2);
        assert!(sent.iter().all(|(_, ch, _)| *ch == CH_REMOVE));
        s.logged_out(b);
        assert_eq!(s.drain(24).len(), 0);
    }

    #[test]
    fn the_server_keeps_within_the_per_tick_limit() {
        let mut s = Server::default();
        s.logged_in(100);
        for p in 0..20u128 {
            s.from_client(p, CH_SKIN, &encode_skin(1, false, Some(&skin(1))));
            s.from_client(p, CH_SKIN, &encode_skin(2, false, Some(&skin(1))));
        }
        assert_eq!(s.drain(24).len(), 24);
        assert_eq!(s.pending(), 16);
        assert_eq!(s.drain(24).len(), 16);
    }

    #[test]
    fn json_strings_and_numbers() {
        let v = json::parse(r#"{"a": "q\"\\\/\n\u0041\ud83d\ude00", "n": -1.5e2, "z": 0}"#).unwrap();
        assert_eq!(v.get("a").unwrap().as_str(), Some("q\"\\/\nA😀"));
        assert_eq!(v.get("n").unwrap().as_f64(), Some(-150.0));
        let w = json::write(&v);
        assert_eq!(json::parse(&w).unwrap(), v);
        assert!(json::parse("{\"a\": tru}").is_err());
        assert!(json::parse("{} x").is_err());
    }
}
