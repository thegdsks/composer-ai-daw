//! Host tests of the C ABI: same functions the worklet calls, with the pinned render hash.

use composer_wasm::{decode, demo_blob, encode, ffi, BlobError};

const SR: u32 = 48_000;
// Same value as PINNED_HASH in core/composer-render/tests/render.rs.
const PINNED_HASH: u64 = 0x9234f99f24d6c39d;

fn fnv(samples: &[f32]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for s in samples {
        for b in s.to_bits().to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    h
}

fn render_through_abi(blob: &[u8]) -> (Vec<f32>, u64) {
    unsafe {
        let p = ffi::cw_alloc(blob.len());
        std::ptr::copy_nonoverlapping(blob.as_ptr(), p, blob.len());
        let h = ffi::cw_create(p, blob.len());
        ffi::cw_dealloc(p, blob.len());
        assert!(!h.is_null());
        let total = ffi::cw_length(h);
        let mut out = Vec::new();
        let mut left = total;
        while left > 0 {
            let n = ffi::cw_process(h, left.min(128) as u32) as usize;
            let l = std::slice::from_raw_parts(ffi::cw_left(h), n);
            let r = std::slice::from_raw_parts(ffi::cw_right(h), n);
            for i in 0..n {
                out.push(l[i]);
                out.push(r[i]);
            }
            left -= n as u64;
        }
        assert_eq!(ffi::cw_position(h), total);
        let clipped = ffi::cw_clipped(h);
        ffi::cw_free(h);
        (out, clipped)
    }
}

#[test]
fn abi_render_matches_pinned_native_hash() {
    let (samples, clipped) = render_through_abi(&demo_blob(SR, 0.25));
    assert_eq!(samples.len(), 408_000);
    assert_eq!(clipped, 0);
    assert_eq!(fnv(&samples), PINNED_HASH, "hash {:#x}", fnv(&samples));
}

#[test]
fn blob_round_trips_and_rejects_garbage() {
    let b = demo_blob(SR, 0.25);
    let d = decode(&b).unwrap();
    assert_eq!(encode(&d), b);
    assert_eq!(decode(&b[..10]), Err(BlobError::Truncated));
    let mut bad = b.clone();
    bad[0] ^= 1;
    assert_eq!(decode(&bad), Err(BlobError::Magic));
    assert_eq!(decode(&b[..b.len() - 1]), Err(BlobError::Truncated));
    unsafe {
        assert!(ffi::cw_create(bad.as_ptr(), bad.len()).is_null());
    }
}

#[test]
fn live_note_through_abi_makes_sound_and_null_is_safe() {
    unsafe {
        let b = demo_blob(SR, 0.25);
        let h = ffi::cw_create(b.as_ptr(), b.len());
        // Skip past the scheduled notes so only the live note can sound.
        for _ in 0..(210_000 / 128) {
            ffi::cw_process(h, 128);
        }
        let peak = |h| {
            ffi::cw_process(h, 128);
            std::slice::from_raw_parts(ffi::cw_left(h), 128)
                .iter()
                .fold(0.0f32, |m, s| m.max(s.abs()))
        };
        assert_eq!(peak(h), 0.0);
        assert_eq!(ffi::cw_note(h, 0, 69, 100, 1), 1);
        assert!(peak(h) > 0.0);
        ffi::cw_free(h);
        assert_eq!(ffi::cw_process(std::ptr::null_mut(), 128), 0);
        assert_eq!(ffi::cw_position(std::ptr::null()), 0);
    }
}
