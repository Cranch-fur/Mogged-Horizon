// Mogged in the game: the edges between Horizon's types and the mod's logic
// (`game::mogged`, runtime.rs). Added to the game as `game::mogged_glue`;
// the helpers (one per hook point) call it.
use crate::game::{Game, mogged as m};
use std::{
    cell::RefCell,
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::mpsc::{Receiver, TryRecvError, channel},
    time::Instant,
};

thread_local! {
    // Presets being downloaded from a URL: the preset (1 or 2), its model,
    // and where the image arrives.
    static DOWNLOADS: RefCell<Vec<(u8, bool, Receiver<Result<Vec<u8>, String>>)>> =
        const { RefCell::new(Vec::new()) };
}

/// Ticks between two tries to give the server our presets.
const RETRY_TICKS: u32 = 40;

/// Time between two looks at the settings file and the presets' skins.
const WATCH_EVERY: std::time::Duration = std::time::Duration::from_secs(1);

/// What was last read, to see what changed since.
#[derive(Default)]
struct Watch {
    /// When it last looked.
    last: Option<Instant>,
    /// The settings file's text (None: there was none to read).
    config: Option<String>,
    /// Each preset's skin: where it came from and, for a file, its bytes' hash.
    presets: [Option<Seen>; m::PRESETS],
}

#[derive(Clone, PartialEq, Eq)]
enum Seen {
    /// A file that is there, by its bytes' hash; a path that can't be read
    /// has no hash.
    File(PathBuf, Option<u64>),
    /// A URL is downloaded once, and again only when it changes.
    Url(String),
}

thread_local! {
    static WATCH: RefCell<Watch> = RefCell::new(Watch::default());
}

fn hash(bytes: &[u8]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

fn config_dir() -> Option<PathBuf> {
    Game::native_config_dir().ok()
}

fn config_file(dir: &std::path::Path) -> PathBuf {
    dir.join(format!("{}.json", m::ID))
}

/// A line in chat, once the player is in a world.
fn message(text: String) {
    log::warn!("{text}");
    m::with_client(|c| c.messages.push(text));
}

/// A PNG's bytes as a 64x64 skin: HD skins scaled down, 64x32 ones converted.
fn decode(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let (w, h, rgba) =
        horizon_client::hud::decode_bytes(bytes).map_err(|_| "not a PNG image".to_owned())?;
    let (h64, rgba) = m::to_64_wide(w, h, &rgba)?;
    horizon_client::skins::process_legacy_skin(64, h64, &rgba).ok_or_else(|| "not a skin".to_owned())
}

/// Our preset `preset` is `result`'s image, or none with the reason told.
fn apply(preset: u8, slim: bool, result: Result<Vec<u8>, String>) {
    match result {
        Ok(rgba) => m::with_client(|c| {
            c.set_own_preset(preset, Some((slim, rgba)));
            // The preset the settings start with, once its image is here.
            if c.config.active == preset && c.me.active == 0 {
                c.me.active = preset;
                c.state_dirty = true;
            }
        }),
        Err(e) => {
            m::with_client(|c| c.set_own_preset(preset, None));
            message(format!("Mogged: preset {preset}: {e}"));
        }
    }
}

/// Reads `config/mogged.json` (written with its defaults the first time) and
/// the presets' skins.
pub(crate) fn load() {
    let Some(dir) = config_dir() else {
        return;
    };
    let _ = std::fs::create_dir_all(dir.join(m::ID));
    let file = config_file(&dir);
    let read = std::fs::read_to_string(&file).ok();
    let config = match &read {
        Some(text) => match m::Config::parse(text) {
            Ok(config) => config,
            Err(e) => {
                message(format!("Mogged: config/{}.json can't be read ({e}); using the defaults", m::ID));
                m::Config::default()
            }
        },
        None => m::Config::default(),
    };
    let text = match read {
        Some(text) => text,
        None => {
            let text = config.to_text();
            let _ = std::fs::write(&file, &text);
            text
        }
    };
    m::with_client(|c| {
        c.config = config.clone();
        c.loaded = true;
    });
    DOWNLOADS.with(|d| d.borrow_mut().clear());
    WATCH.with(|w| {
        let mut w = w.borrow_mut();
        w.config = Some(text);
        w.presets = Default::default();
        w.last = Some(Instant::now());
    });
    for i in 0..m::PRESETS {
        load_preset(&dir, &config, i, None);
    }
    m::with_client(|c| c.restore_active());
}

/// Preset `i`'s skin as `config` names it, unless it is the one already
/// `seen`; a file's bytes are compared by their hash, so a changed image is
/// read again and one that stays the same costs only the read.
fn load_preset(dir: &std::path::Path, config: &m::Config, i: usize, seen: Option<&Seen>) {
    let n = i as u8 + 1;
    let preset = &config.presets[i];
    let (now, bytes) = match m::source(dir, i, &preset.skin) {
        m::Source::None => (None, None),
        m::Source::File(path) => match std::fs::read(&path) {
            Ok(bytes) => (Some(Seen::File(path, Some(hash(&bytes)))), Some(Ok(bytes))),
            Err(e) => {
                let error = format!("{}: {e}", path.display());
                (Some(Seen::File(path, None)), Some(Err(error)))
            }
        },
        m::Source::Url(url) => (Some(Seen::Url(url)), None),
    };
    if seen.is_some() && now.as_ref() == seen {
        return;
    }
    if seen.is_some() {
        // Whatever was waiting for the old one isn't wanted any more.
        DOWNLOADS.with(|d| d.borrow_mut().retain(|(p, _, _)| *p != n));
    }
    WATCH.with(|w| w.borrow_mut().presets[i] = now.clone());
    match (now, bytes) {
        (None, _) => m::with_client(|c| c.set_own_preset(n, None)),
        (Some(Seen::File(..)), Some(result)) => apply(n, preset.slim, result.and_then(|b| decode(&b))),
        (Some(Seen::Url(url)), _) => {
            m::with_client(|c| c.set_own_preset(n, None));
            let (send, receive) = channel();
            std::thread::spawn(move || {
                let result = horizon_client::skins::mogged_access::http_get(&url)
                    .ok_or_else(|| format!("{url} could not be downloaded"))
                    .and_then(|bytes| decode(&bytes));
                let _ = send.send(result);
            });
            DOWNLOADS.with(|d| d.borrow_mut().push((n, preset.slim, receive)));
        }
        (Some(Seen::File(..)), None) => {}
    }
}

/// About once a second: the settings file read again when it changed (all
/// of it), else each preset's skin when it appeared, went or changed.
fn watch() {
    let Some(dir) = config_dir() else {
        return;
    };
    let text = std::fs::read_to_string(config_file(&dir)).ok();
    if text.is_none() || WATCH.with(|w| w.borrow().config != text) {
        load();
        return;
    }
    let config = m::with_client(|c| c.config.clone());
    for i in 0..m::PRESETS {
        let seen = WATCH.with(|w| w.borrow().presets[i].clone());
        match seen {
            Some(seen) => load_preset(&dir, &config, i, Some(&seen)),
            // Nothing was there: anything now is new.
            None => {
                if !matches!(m::source(&dir, i, &config.presets[i].skin), m::Source::None) {
                    load_preset(&dir, &config, i, None);
                }
            }
        }
    }
}

/// Presets that finished downloading.
fn poll_downloads() {
    let done: Vec<(u8, bool, Result<Vec<u8>, String>)> = DOWNLOADS.with(|d| {
        let mut done = Vec::new();
        d.borrow_mut().retain(|(n, slim, receive)| match receive.try_recv() {
            Ok(result) => {
                done.push((*n, *slim, result));
                false
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => false,
        });
        done
    });
    for (n, slim, result) in done {
        apply(n, slim, result);
    }
}

fn save(text: String) {
    let Some(dir) = config_dir() else {
        return;
    };
    // Our own write isn't a change to read back.
    WATCH.with(|w| w.borrow_mut().config = Some(text.clone()));
    if let Err(e) = std::fs::write(config_file(&dir), text) {
        message(format!("Mogged: config/{}.json can't be saved: {e}", m::ID));
    }
}

/// Once a tick, in the window's game and in the server's.
pub(crate) fn tick(game: &mut Game<'_>) {
    if !game.native_window() {
        server_tick(game);
        return;
    }
    if !game.native_on(m::ID) {
        return;
    }
    if !m::with_client(|c| c.loaded) {
        load();
    }
    if game.native_player().is_none() {
        m::with_client(|c| {
            if c.synced || !c.others.is_empty() || c.me.anim.is_some() {
                c.left_world();
            }
        });
        return;
    }
    for line in m::with_client(|c| std::mem::take(&mut c.messages)) {
        game.native_chat(&line);
    }
    // Give the server our presets once a join (again after a failure), and
    // each switch after that, with its animation.
    let (sync, state) = m::with_client(|c| {
        if !c.synced {
            if c.retry > 0 {
                c.retry -= 1;
                (None, None)
            } else {
                (Some(c.sync_messages()), None)
            }
        } else if c.state_dirty {
            c.state_dirty = false;
            (None, Some(m::encode_state(c.own_state(true))))
        } else {
            (None, None)
        }
    });
    if let Some(messages) = sync {
        // On a server without the mod (or without mod messages) the first
        // send fails and nothing else is tried until the next retry.
        let ok = messages
            .iter()
            .all(|(channel, data)| game.native_net_send_to_server(channel, data).is_ok());
        m::with_client(|c| {
            if ok {
                c.synced = true;
                c.state_dirty = false;
            } else {
                c.retry = RETRY_TICKS;
            }
        });
    }
    if let Some(data) = state
        && game.native_net_send_to_server(m::CH_STATE, &data).is_err()
    {
        m::with_client(|c| {
            c.synced = false;
            c.retry = 0;
        });
    }
}

/// The switch key.
pub(crate) fn key(game: &mut Game<'_>, key: &str, pressed: bool) {
    if !pressed || !game.native_window() || !game.native_on(m::ID) {
        return;
    }
    if !game.native_playing() || game.native_player().is_none() {
        return;
    }
    if key != game.native_mod_key(m::ID, m::KEY, m::DEFAULT_KEY) {
        return;
    }
    let switched = m::with_client(|c| {
        if !c.loaded {
            return Err(false);
        }
        match c.toggle(Instant::now()) {
            Some(_) => Ok(c.config.to_text()),
            None => Err(c.me.presets.iter().all(Option::is_none)),
        }
    });
    match switched {
        Ok(text) => save(text),
        Err(true) => game.native_chat(&format!(
            "Mogged: no presets yet. Put preset1.png and preset2.png in config/{}/ (Options > Mods > Mogged > Open skins folder), or set them in config/{}.json; they load by themselves.",
            m::ID,
            m::ID
        )),
        Err(false) => {}
    }
}

/// Once a frame: the sweeps' textures, and their sound.
pub(crate) fn frame(game: &mut Game<'_>) {
    if !game.native_window() || !game.native_on(m::ID) {
        return;
    }
    let now = Instant::now();
    // The skins are watched here, not in the tick: frames go on in menus, on
    // the title screen and while the game is paused (Options, where the
    // settings page is), ticks don't.
    if !m::with_client(|c| c.loaded) {
        load();
    }
    let due = WATCH.with(|w| {
        let mut w = w.borrow_mut();
        let due = w.last.is_none_or(|last| now.duration_since(last) >= WATCH_EVERY);
        if due {
            w.last = Some(now);
        }
        due
    });
    if due || DOWNLOADS.with(|d| !d.borrow().is_empty()) {
        let before = m::with_client(|c| c.page());
        if due {
            watch();
        }
        poll_downloads();
        if m::with_client(|c| c.page()) != before {
            refresh_page(game);
        }
    }
    for (player, location) in m::with_client(|c| c.defaults_wanted()) {
        let pixels = game.skin_downloads.borrow().mogged_pixels(&location);
        m::with_client(|c| c.set_default_pixels(player, pixels));
    }
    let (frames, started) = m::with_client(|c| c.frames(now));
    for (location, rgba) in frames {
        game.store_skin(location.clone(), rgba);
        game.skin_downloads.borrow_mut().drawn(&location);
    }
    let own = m::with_client(|c| c.own);
    if started.iter().any(|p| Some(*p) == own)
        && let Some(p) = game.native_player()
    {
        let at = [p.x, p.y + 1.0, p.z];
        game.native_play_local_sound("minecraft:entity.illusioner.mirror_move", at, 0.7, 1.35, 0);
        game.native_play_local_sound("minecraft:block.amethyst_block.chime", at, 1.0, 0.8, 0);
    }
}

/// `drawn_skin`'s answer for `player`, whose own skin is `value`: the
/// preset's texture while one is on.
pub(crate) fn drawn_skin(
    game: &mut Game<'_>,
    own: Option<u128>,
    player: u128,
    value: (String, bool),
) -> (String, bool) {
    if !game.native_on(m::ID) {
        return value;
    }
    let display = m::with_client(|c| {
        if own.is_some() && c.own != own {
            c.own = own;
        }
        c.display(player, &value, Instant::now())
    });
    let m::Display::Texture { location, slim } = display else {
        return value;
    };
    let stored = game.skin_slots.contains(&location) && game.gui.atlas.sprites.contains_key(&location);
    if !stored {
        // The sweep's texture comes back with the next frame; a preset's now.
        let Some(pixels) = m::with_client(|c| c.pixels(&location).map(<[u8]>::to_vec)) else {
            return value;
        };
        game.store_skin(location.clone(), pixels);
        if !game.gui.atlas.sprites.contains_key(&location) {
            // Every skin slot is in use this frame.
            return value;
        }
    }
    game.skin_downloads.borrow_mut().drawn(&location);
    (location, slim)
}

/// A message from the server (window's game).
pub(crate) fn from_server(game: &mut Game<'_>, channel: &str, data: &[u8]) {
    if !game.native_window() || !channel.starts_with("mogged:") {
        return;
    }
    m::with_client(|c| c.from_server(channel, data, Instant::now()));
}

/// A message from a player's client (server's game).
pub(crate) fn from_client(player: u128, channel: &str, data: &[u8]) {
    if channel.starts_with("mogged:") {
        m::with_server(|s| s.from_client(player, channel, data));
    }
}

pub(crate) fn logged_in(player: u128) {
    m::with_server(|s| s.logged_in(player));
}

pub(crate) fn logged_out(player: u128) {
    m::with_server(|s| s.logged_out(player));
}

fn server_tick(game: &mut Game<'_>) {
    for (to, channel, data) in m::with_server(|s| s.drain(m::SERVER_SENDS_PER_TICK)) {
        // A player who left, or one without Horizon, simply doesn't get it.
        let _ = game.native_net_send_to_player(to, channel, &data);
    }
}

/// Mogged's row in Options > Mods, with its key.
pub(crate) fn listed(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    mods.push(crate::game::native_hooks::NativeMod {
        id: m::ID.into(),
        title: format!("Mogged {}", m::VERSION),
        description: m::DESCRIPTION.into(),
        keys: vec![[m::KEY.into(), m::KEY_LABEL.into(), m::DEFAULT_KEY.into()]],
    });
}

thread_local! {
    /// The settings page's buttons as last shown, for its clicks.
    static PAGE: RefCell<Vec<m::Button>> = const { RefCell::new(Vec::new()) };
}

/// The settings page shows what changed (a preset that got or lost its skin),
/// even while it is open.
fn refresh_page(game: &mut Game<'_>) {
    let i = game.settings.mods.installed.iter().position(|row| row.id == m::ID);
    if let Some(i) = i {
        game.native_settings_refresh(i);
    }
}

/// Mogged's settings page.
pub(crate) fn settings(mod_id: &str, buttons: &mut Vec<String>) {
    if mod_id != m::ID {
        return;
    }
    if !m::with_client(|c| c.loaded) {
        load();
    }
    let page = m::with_client(|c| c.page());
    PAGE.with(|p| *p.borrow_mut() = page.iter().map(|(b, _)| *b).collect());
    // The skins' places are text, not buttons (notes.rs draws them so).
    buttons.extend(page.into_iter().map(|(button, label)| match button {
        m::Button::Place(_) => format!("\u{1}{label}"),
        _ => label,
    }));
}

/// A click on Mogged's settings page.
pub(crate) fn click(mod_id: &str, button: usize) {
    if mod_id != m::ID {
        return;
    }
    let Some(button) = PAGE.with(|p| p.borrow().get(button).copied()) else {
        return;
    };
    let (result, text) = m::with_client(|c| {
        let result = c.config.click(button);
        match result {
            m::Click::SaveAndResend => {
                if let m::Button::Model(n) = button {
                    let slim = c.config.presets[usize::from(n) - 1].slim;
                    c.set_own_slim(n, slim);
                }
            }
            m::Click::Wear(n) => {
                if c.me.active != n && c.me.has_content(n) {
                    c.select(n, Instant::now());
                }
            }
            _ => {}
        }
        (result, c.config.to_text())
    });
    match result {
        m::Click::Save | m::Click::SaveAndResend | m::Click::Wear(_) => {
            save(text);
            // The animation and effects travel with the state.
            m::with_client(|c| c.state_dirty = true);
        }
        m::Click::OpenFolder => {
            if let Some(dir) = config_dir() {
                let folder = dir.join(m::ID);
                let _ = std::fs::create_dir_all(&folder);
                crate::game::packs::open_folder(&folder);
            }
        }
        m::Click::Nothing => {}
    }
}
