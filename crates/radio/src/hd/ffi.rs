//! libnrsc5 (theori-io/nrsc5), loaded at run time so the receiver still starts without it.
//! Layouts mirror `include/nrsc5.h`; only the events this receiver uses are read.

use libloading::Library;
use std::ffi::{c_char, c_int, c_uint, c_void, CStr};
use std::sync::OnceLock;

/// `NRSC5_SAMPLE_RATE_NATIVE_FM`: complex samples per second `pipe_samples_cf32` expects.
pub const SAMPLE_RATE: f64 = 744_187.5;
/// `NRSC5_SAMPLE_RATE_AUDIO`: decoded audio is interleaved stereo i16 at this rate.
pub const AUDIO_RATE: u32 = 44_100;

const MIME_STATION_LOGO: u32 = 0xD9C7_2536;
const MIME_PRIMARY_IMAGE: u32 = 0xBE4B_7536;
const AUDIO_FLAGS_UNAVAILABLE: c_uint = 1;
const HERE_IMAGE_TRAFFIC: c_int = 0;

// event numbers: position in the header's event enum
const EVENT_SYNC: c_uint = 2;
const EVENT_LOST_SYNC: c_uint = 3;
const EVENT_MER: c_uint = 4;
const EVENT_BER: c_uint = 5;
const EVENT_AUDIO: c_uint = 7;
const EVENT_ID3: c_uint = 8;
const EVENT_LOT: c_uint = 10;
const EVENT_AUDIO_SERVICE: c_uint = 14;
const EVENT_STATION_NAME: c_uint = 16;
const EVENT_STATION_SLOGAN: c_uint = 17;
const EVENT_STATION_MESSAGE: c_uint = 18;
const EVENT_HERE_IMAGE: c_uint = 23;
const EVENT_LOT_HEADER: c_uint = 24;
const EVENT_LOT_FRAGMENT: c_uint = 25;

/// The decoded events, borrowed from the library for the length of one callback.
pub enum Event<'a> {
    Sync,
    LostSync,
    /// Modulation error ratio of each sideband, dB.
    Mer(f32, f32),
    Ber(f32),
    /// Interleaved stereo; `None` while the program has no digital audio.
    Audio { program: u32, pcm: Option<&'a [i16]> },
    Id3 { program: u32, title: Option<String>, artist: Option<String>, album: Option<String>, art_lot: Option<u32> },
    Lot { lot: u32, name: String, logo: bool, data: &'a [u8] },
    /// A logo or album-art file has started: its total `size` in bytes.
    LotHeader { lot: u32, size: u32 },
    /// More of a logo or album-art file: `got` bytes so far.
    LotFragment { lot: u32, got: u32 },
    Program(u32),
    Name(String),
    Slogan(String),
    Message(String),
    /// HERE traffic map tile (`part` of 9, row-major) or weather map.
    HereImage { traffic: bool, part: u32, name: String, data: &'a [u8] },
}

#[repr(C)]
struct Mer {
    lower: f32,
    upper: f32,
}

#[repr(C)]
struct Audio {
    program: c_uint,
    data: *const i16,
    count: usize,
    flags: c_uint,
}

#[repr(C)]
struct Id3 {
    program: c_uint,
    title: *const c_char,
    artist: *const c_char,
    album: *const c_char,
    genre: *const c_char,
    ufid_owner: *const c_char,
    ufid_id: *const c_char,
    xhdr_mime: u32,
    xhdr_param: c_int,
    xhdr_lot: c_int,
}

#[repr(C)]
struct Lot {
    port: u16,
    lot: c_uint,
    size: c_uint,
    mime: u32,
    name: *const c_char,
    data: *const u8,
    expiry_utc: *const c_void,
    service: *const c_void,
    component: *const Component,
}

#[repr(C)]
struct LotFragment {
    lot: c_uint,
    seq: c_uint,
    repeat: c_uint,
    size: c_uint,
    bytes_so_far: c_uint,
    is_duplicate: c_int,
    data: *const u8,
    service: *const c_void,
    component: *const Component,
}

/// A logo or album art, the pictures the panel waits for.
unsafe fn picture(c: *const Component) -> bool {
    !c.is_null() && matches!((*c).mime, MIME_STATION_LOGO | MIME_PRIMARY_IMAGE)
}

/// `nrsc5_sig_component_t` with its data-service union member.
#[repr(C)]
struct Component {
    next: *const Component,
    kind: u8,
    id: u8,
    _pad: u16,
    port: u16,
    service_data_type: u16,
    aas_type: u8,
    mime: u32,
}

#[repr(C)]
struct HereImage {
    image_type: c_int,
    seq: c_int,
    n1: c_int,
    n2: c_int,
    time_utc: *const c_void,
    latitude1: f32,
    longitude1: f32,
    latitude2: f32,
    longitude2: f32,
    name: *const c_char,
    size: c_uint,
    data: *const u8,
}

/// The event union follows a 4-byte tag and has pointer alignment.
unsafe fn body<'a, T>(evt: *const c_void) -> &'a T {
    &*((evt as *const u8).add(8) as *const T)
}

unsafe fn text(p: *const c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let s = CStr::from_ptr(p).to_string_lossy().trim().to_string();
    (!s.is_empty()).then_some(s)
}

unsafe fn bytes<'a>(p: *const u8, n: c_uint) -> &'a [u8] {
    if p.is_null() {
        &[]
    } else {
        std::slice::from_raw_parts(p, n as usize)
    }
}

unsafe fn parse<'a>(evt: *const c_void) -> Option<Event<'a>> {
    let kind = *(evt as *const c_uint);
    Some(match kind {
        EVENT_SYNC => Event::Sync,
        EVENT_LOST_SYNC => Event::LostSync,
        EVENT_MER => {
            let m: &Mer = body(evt);
            Event::Mer(m.lower, m.upper)
        }
        EVENT_BER => Event::Ber(*body::<f32>(evt)),
        EVENT_AUDIO => {
            let a: &Audio = body(evt);
            let ok = a.flags & AUDIO_FLAGS_UNAVAILABLE == 0 && !a.data.is_null();
            Event::Audio { program: a.program, pcm: ok.then(|| std::slice::from_raw_parts(a.data, a.count)) }
        }
        EVENT_ID3 => {
            let i: &Id3 = body(evt);
            let art_lot = (i.xhdr_mime == MIME_PRIMARY_IMAGE && i.xhdr_lot >= 0).then_some(i.xhdr_lot as u32);
            Event::Id3 { program: i.program, title: text(i.title), artist: text(i.artist), album: text(i.album), art_lot }
        }
        EVENT_LOT => {
            let l: &Lot = body(evt);
            let logo = !l.component.is_null() && (*l.component).mime == MIME_STATION_LOGO;
            Event::Lot { lot: l.lot, name: text(l.name).unwrap_or_default(), logo, data: bytes(l.data, l.size) }
        }
        EVENT_LOT_HEADER => {
            let l: &Lot = body(evt);
            if !picture(l.component) {
                return None;
            }
            Event::LotHeader { lot: l.lot, size: l.size }
        }
        EVENT_LOT_FRAGMENT => {
            let f: &LotFragment = body(evt);
            if !picture(f.component) {
                return None;
            }
            Event::LotFragment { lot: f.lot, got: f.bytes_so_far }
        }
        EVENT_AUDIO_SERVICE => Event::Program(*body::<c_uint>(evt)),
        EVENT_STATION_NAME => Event::Name(text(*body::<*const c_char>(evt))?),
        EVENT_STATION_SLOGAN => Event::Slogan(text(*body::<*const c_char>(evt))?),
        EVENT_STATION_MESSAGE => Event::Message(text(*body::<*const c_char>(evt))?),
        EVENT_HERE_IMAGE => {
            let h: &HereImage = body(evt);
            Event::HereImage { traffic: h.image_type == HERE_IMAGE_TRAFFIC, part: h.n1.max(0) as u32, name: text(h.name).unwrap_or_default(), data: bytes(h.data, h.size) }
        }
        _ => return None,
    })
}

type Callback = unsafe extern "C" fn(*const c_void, *mut c_void);

struct Lib {
    _lib: Library,
    open_pipe: unsafe extern "C" fn(*mut *mut c_void) -> c_int,
    set_callback: unsafe extern "C" fn(*mut c_void, Callback, *mut c_void),
    pipe_cf32: unsafe extern "C" fn(*mut c_void, *const f32, c_uint) -> c_int,
    close: unsafe extern "C" fn(*mut c_void),
}

/// Where libnrsc5 is looked for: the loader's own path first, then `~/.local/lib`.
fn candidates() -> Vec<String> {
    let mut v = vec!["libnrsc5.so".to_string()];
    if let Some(home) = dirs::home_dir() {
        v.push(home.join(".local/lib/libnrsc5.so").display().to_string());
    }
    v
}

fn lib() -> Result<&'static Lib, String> {
    static LIB: OnceLock<Result<Lib, String>> = OnceLock::new();
    LIB.get_or_init(|| unsafe {
        let lib = candidates()
            .into_iter()
            .find_map(|p| Library::new(p).ok())
            .ok_or_else(|| "HD Radio needs libnrsc5 — build nrsc5 and copy libnrsc5.so to ~/.local/lib".to_string())?;
        let sym = |name: &[u8]| -> Result<*const c_void, String> {
            lib.get::<*const c_void>(name).map(|s| *s).map_err(|e| format!("libnrsc5: {e}"))
        };
        Ok(Lib {
            open_pipe: std::mem::transmute(sym(b"nrsc5_open_pipe")?),
            set_callback: std::mem::transmute(sym(b"nrsc5_set_callback")?),
            pipe_cf32: std::mem::transmute(sym(b"nrsc5_pipe_samples_cf32")?),
            close: std::mem::transmute(sym(b"nrsc5_close")?),
            _lib: lib,
        })
    })
    .as_ref()
    .map_err(|e| e.clone())
}

type Sink = Box<dyn FnMut(Event) + Send>;

unsafe extern "C" fn callback(evt: *const c_void, opaque: *mut c_void) {
    if let Some(e) = parse(evt) {
        (*(opaque as *mut Sink))(e);
    }
}

/// One decoder in pipe mode: samples go in on the caller's thread and events come back on it.
pub struct Session {
    st: *mut c_void,
    sink: *mut Sink,
}

// the session is only ever used from one thread at a time
unsafe impl Send for Session {}

impl Session {
    pub fn open(sink: impl FnMut(Event) + Send + 'static) -> Result<Self, String> {
        let lib = lib()?;
        let mut st = std::ptr::null_mut();
        unsafe {
            if (lib.open_pipe)(&mut st) != 0 || st.is_null() {
                return Err("libnrsc5 would not open".into());
            }
            let sink: *mut Sink = Box::into_raw(Box::new(Box::new(sink)));
            (lib.set_callback)(st, callback, sink as *mut c_void);
            Ok(Session { st, sink })
        }
    }

    /// `iq` is interleaved I/Q at `SAMPLE_RATE`.
    pub fn push(&mut self, iq: &[f32]) {
        if let Ok(lib) = lib() {
            unsafe {
                (lib.pipe_cf32)(self.st, iq.as_ptr(), (iq.len() & !1) as c_uint);
            }
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Ok(lib) = lib() {
            unsafe {
                (lib.close)(self.st);
                drop(Box::from_raw(self.sink));
            }
        }
    }
}
