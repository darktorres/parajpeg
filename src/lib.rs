//! parajpeg's CPU decoder for Rust: a parallel JPEG decoder whose output is bit-identical to libjpeg-turbo's default decode
//! (upstream's README and benchmarks; this crate only wraps its CPU backend).
//!
//! The API is two calls: [`header`] reads an image's size, and [`decode_rgb`] decodes it into a buffer the caller
//! sized from the header. Anything parajpeg doesn't decode (CMYK, arithmetic coding, a damaged or truncated stream)
//! gives `None` or `false`, and the caller decodes it another way.

use std::ffi::c_int;

unsafe extern "C" {
    fn parajpeg_shim_header(data: *const u8, size: usize, width: *mut u32, height: *mut u32, components: *mut c_int)
    -> c_int;
    fn parajpeg_shim_decode_rgb(data: *const u8, size: usize, threads: c_int, dst: *mut u8, stride: usize) -> c_int;
}

/// An image's size, from [`header`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub width: u32,
    pub height: u32,
}

impl Header {
    /// The packed RGB buffer [`decode_rgb`] fills: three bytes a pixel, rows one after another.
    pub fn rgb_len(self) -> usize {
        usize::try_from(self.width).unwrap_or(usize::MAX) * usize::try_from(self.height).unwrap_or(usize::MAX) * 3
    }
}

/// The size of a three-component JPEG that parajpeg reads the headers of, or `None` for anything else.
pub fn header(bytes: &[u8]) -> Option<Header> {
    let (mut width, mut height, mut components): (u32, u32, c_int) = (0, 0, 0);
    // SAFETY: `bytes` is valid for its length, and the three out-parameters are valid for the writes the shim makes.
    let status = unsafe {
        parajpeg_shim_header(bytes.as_ptr(), bytes.len(), &raw mut width, &raw mut height, &raw mut components)
    };
    (status == 0 && components == 3 && width > 0 && height > 0).then_some(Header { width, height })
}

/// Decodes `bytes` as packed RGB into `out`, which must be [`Header::rgb_len`] bytes, on `threads` threads (0: the
/// hardware's count). Returns `false` on any failure, and then `out` holds nothing usable.
pub fn decode_rgb(bytes: &[u8], threads: usize, header: Header, out: &mut [u8]) -> bool {
    if out.len() != header.rgb_len() {
        return false;
    }
    let threads = c_int::try_from(threads).unwrap_or(c_int::MAX);
    let stride = usize::try_from(header.width).unwrap_or(usize::MAX) * 3;
    // SAFETY: `bytes` and `out` are valid for their lengths, and the decoder writes `height` rows of `stride` bytes,
    // which is `out.len()` as checked above.
    unsafe { parajpeg_shim_decode_rgb(bytes.as_ptr(), bytes.len(), threads, out.as_mut_ptr(), stride) == 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 64 × 48 baseline JPEG, 4:2:0, made with ImageMagick.
    const SMALL: &[u8] = include_bytes!("../tests/fixtures/small-420.jpg");

    #[test]
    fn reads_the_size_of_a_jpeg() {
        assert_eq!(header(SMALL), Some(Header { width: 64, height: 48 }));
    }

    #[test]
    fn rejects_what_is_not_a_jpeg() {
        assert_eq!(header(b""), None);
        assert_eq!(header(b"\x89PNG\r\n\x1a\n not a jpeg"), None);
        assert_eq!(header(&SMALL[..40]), None);
    }

    #[test]
    fn decodes_into_a_buffer_of_the_header_size() {
        let head = header(SMALL).expect("the fixture's header");
        let mut out = vec![0u8; head.rgb_len()];
        assert!(decode_rgb(SMALL, 2, head, &mut out));
        assert!(out.iter().any(|&b| b != 0), "the decode wrote pixels");
    }

    #[test]
    fn a_buffer_of_the_wrong_size_is_refused() {
        let head = header(SMALL).expect("the fixture's header");
        let mut out = vec![0u8; head.rgb_len() - 1];
        assert!(!decode_rgb(SMALL, 1, head, &mut out));
    }

    #[test]
    fn decodes_on_several_threads_at_once_all_finish() {
        // parajpeg's shared thread pool runs one parallel job at a time; without the shim's lock, these hang. The
        // 640 × 480 picture is large enough for its parallel path.
        const PLASMA: &[u8] = include_bytes!("../tests/fixtures/plasma-420.jpg");
        let head = header(PLASMA).expect("the fixture's header");
        let workers: Vec<_> = (0..4)
            .map(|_| {
                std::thread::spawn(move || {
                    let mut out = vec![0u8; head.rgb_len()];
                    (0..20).all(|_| decode_rgb(PLASMA, 3, head, &mut out))
                })
            })
            .collect();
        for worker in workers {
            assert!(worker.join().expect("the worker didn't panic"));
        }
    }

    #[test]
    fn a_truncated_stream_is_refused_or_decoded_without_panicking() {
        let head = header(SMALL).expect("the fixture's header");
        let mut out = vec![0u8; head.rgb_len()];
        // Either outcome is fine; the point is that it returns.
        let _ = decode_rgb(&SMALL[..SMALL.len() / 2], 1, head, &mut out);
    }
}
