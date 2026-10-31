//! mbelib (szechyjs/mbelib, Arch package `mbelib`), loaded at run time so the receiver still
//! starts without it. Decodes IMBE, the P25 Phase 1 voice codec. `Parms` mirrors
//! `struct mbe_parameters` in `mbelib.h`; recheck it if mbelib is upgraded.

use libloading::Library;
use std::ffi::{c_char, c_int, c_short};
use std::sync::OnceLock;

/// Samples per IMBE frame (20 ms at 8 kHz).
pub const FRAME_SAMPLES: usize = 160;
pub const RATE: u32 = 8_000;
/// mbelib's unvoiced-synthesis quality, as DSD uses it.
const UV_QUALITY: c_int = 3;

#[repr(C)]
struct Parms {
    w0: f32,
    l: c_int,
    k: c_int,
    vl: [c_int; 57],
    ml: [f32; 57],
    log2ml: [f32; 57],
    phil: [f32; 57],
    psil: [f32; 57],
    gamma: f32,
    un: c_int,
    repeat: c_int,
}

type Imbe = unsafe extern "C" fn(*mut c_short, *mut c_int, *mut c_int, *mut c_char, *mut [c_char; 23], *mut c_char, *mut Parms, *mut Parms, *mut Parms, c_int);
type Init = unsafe extern "C" fn(*mut Parms, *mut Parms, *mut Parms);

struct Lib {
    imbe: Imbe,
    init: Init,
    _lib: Library,
}

fn lib() -> Result<&'static Lib, String> {
    static LIB: OnceLock<Result<Lib, String>> = OnceLock::new();
    LIB.get_or_init(|| unsafe {
        let lib = ["libmbe.so.1", "libmbe.so"]
            .into_iter()
            .find_map(|p| Library::new(p).ok())
            .ok_or_else(|| "P25 voice needs mbelib — sudo pacman -S mbelib".to_string())?;
        let imbe = *lib.get::<Imbe>(b"mbe_processImbe7200x4400Frame").map_err(|e| format!("mbelib: {e}"))?;
        let init = *lib.get::<Init>(b"mbe_initMbeParms").map_err(|e| format!("mbelib: {e}"))?;
        Ok(Lib { imbe, init, _lib: lib })
    })
    .as_ref()
    .map_err(|e| e.clone())
}

/// Whether mbelib loads; the error says how to install it.
pub fn available() -> Result<(), String> {
    lib().map(|_| ())
}

/// One call's codec state. Start a new one for each call.
pub struct Decoder {
    lib: &'static Lib,
    parms: Box<[Parms; 3]>,
}

// mbelib keeps no global state; the parameter blocks are owned here
unsafe impl Send for Decoder {}

impl Decoder {
    pub fn new() -> Result<Self, String> {
        let lib = lib()?;
        // SAFETY: Parms is plain old data; zeroed is a valid start before init
        let mut parms: Box<[Parms; 3]> = Box::new(unsafe { std::mem::zeroed() });
        let [a, b, c] = &mut *parms;
        unsafe { (lib.init)(a, b, c) };
        Ok(Decoder { lib, parms })
    }

    /// Decode one 144-bit coded frame (as `imbe_fr[8][23]` bits). Returns 8 kHz audio and the
    /// number of bit errors the codec corrected.
    pub fn decode(&mut self, frame: &[[u8; 23]; 8]) -> ([f32; FRAME_SAMPLES], u32) {
        let mut fr: [[c_char; 23]; 8] = [[0; 23]; 8];
        for (o, i) in fr.iter_mut().zip(frame) {
            for (a, &b) in o.iter_mut().zip(i) {
                *a = b as c_char;
            }
        }
        let mut out = [0 as c_short; FRAME_SAMPLES];
        let (mut errs, mut errs2) = (0 as c_int, 0 as c_int);
        // mbelib writes one character per corrected error, unbounded
        let mut err_str = [0 as c_char; 1024];
        let mut data = [0 as c_char; 88];
        let [a, b, c] = &mut *self.parms;
        unsafe {
            (self.lib.imbe)(out.as_mut_ptr(), &mut errs, &mut errs2, err_str.as_mut_ptr(), fr.as_mut_ptr(), data.as_mut_ptr(), a, b, c, UV_QUALITY);
        }
        let mut pcm = [0f32; FRAME_SAMPLES];
        for (p, &s) in pcm.iter_mut().zip(&out) {
            *p = s as f32 / 32768.0;
        }
        (pcm, errs2.max(0) as u32)
    }
}
