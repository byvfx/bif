//! FFI allocation safety guards.
//!
//! USD scenes can author counts (vertices, instances, time samples) that, when
//! multiplied by a stride, overflow `usize` or request many gigabytes — feeding
//! them directly to `Vec::with_capacity` or `std::slice::from_raw_parts` aborts
//! the process via the global allocator. These helpers bound-check counts at
//! the FFI boundary and return `UsdBridgeError::AllocTooLarge` instead.

use super::cpp_bridge::{UsdBridgeError, UsdBridgeResult};

/// Maximum bytes a single FFI-driven allocation may request. 4 GiB is generous
/// for any single attribute we currently surface (largest realistic case is
/// dense per-frame instancer transforms); raise when streaming lands.
pub const MAX_ALLOC_BYTES: usize = 4 << 30;

/// Multiply two counts, returning `AllocTooLarge` on overflow rather than
/// wrapping silently. `context` is included verbatim in the error so users can
/// see which prim/attribute tripped the guard.
#[inline]
pub fn checked_mul_count(a: usize, b: usize, context: &str) -> UsdBridgeResult<usize> {
    a.checked_mul(b)
        .ok_or_else(|| UsdBridgeError::AllocTooLarge {
            context: format!("{context} (overflow: {a} * {b})"),
            requested_bytes: usize::MAX,
            max_bytes: MAX_ALLOC_BYTES,
        })
}

/// Verify that allocating `count` elements of `T` stays under `MAX_ALLOC_BYTES`.
/// Returns the count back on success so it can be plumbed straight into
/// `Vec::with_capacity` / `from_raw_parts`.
#[inline]
pub fn checked_alloc_count<T>(count: usize, context: &str) -> UsdBridgeResult<usize> {
    let bytes = count.checked_mul(std::mem::size_of::<T>()).ok_or_else(|| {
        UsdBridgeError::AllocTooLarge {
            context: format!(
                "{context} (overflow: {count} * {})",
                std::mem::size_of::<T>()
            ),
            requested_bytes: usize::MAX,
            max_bytes: MAX_ALLOC_BYTES,
        }
    })?;
    if bytes > MAX_ALLOC_BYTES {
        return Err(UsdBridgeError::AllocTooLarge {
            context: context.to_string(),
            requested_bytes: bytes,
            max_bytes: MAX_ALLOC_BYTES,
        });
    }
    Ok(count)
}

/// Soft variant for infallible converters that cannot propagate `Result`:
/// returns `Some(count * stride)` only when the multiply succeeds AND the
/// resulting `count * stride * sizeof::<T>()` stays under `MAX_ALLOC_BYTES`.
/// Logs and returns `None` otherwise — caller should fall back to an empty
/// vector (i.e. drop the offending attribute).
#[inline]
pub fn safe_mul_count<T>(count: usize, stride: usize, context: &str) -> Option<usize> {
    let total = count.checked_mul(stride)?;
    let bytes = total.checked_mul(std::mem::size_of::<T>())?;
    if bytes > MAX_ALLOC_BYTES {
        log::warn!(
            "ffi_guard: dropping {context} ({count} * {stride} * {}B = {bytes} > {MAX_ALLOC_BYTES})",
            std::mem::size_of::<T>()
        );
        return None;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bif_math::Mat4;

    #[test]
    fn checked_mul_count_happy() {
        assert_eq!(checked_mul_count(10, 20, "ctx").unwrap(), 200);
    }

    #[test]
    fn checked_mul_count_overflow() {
        let err = checked_mul_count(usize::MAX, 2, "ctx").unwrap_err();
        match err {
            UsdBridgeError::AllocTooLarge { context, .. } => assert!(context.contains("overflow")),
            other => panic!("expected AllocTooLarge, got {other:?}"),
        }
    }

    #[test]
    fn checked_alloc_count_happy() {
        assert_eq!(checked_alloc_count::<f32>(1024, "ctx").unwrap(), 1024);
    }

    #[test]
    fn checked_alloc_count_too_large() {
        // 16_000 * 25_000 Mat4 (64 B) = ~25 GiB — the rt_010_base.usda case.
        let err = checked_alloc_count::<Mat4>(16_000 * 25_000, "instancer_animation").unwrap_err();
        match err {
            UsdBridgeError::AllocTooLarge {
                context,
                requested_bytes,
                max_bytes,
            } => {
                assert_eq!(context, "instancer_animation");
                assert!(requested_bytes > max_bytes);
                assert_eq!(max_bytes, MAX_ALLOC_BYTES);
            }
            other => panic!("expected AllocTooLarge, got {other:?}"),
        }
    }

    #[test]
    fn checked_alloc_count_overflow() {
        let err = checked_alloc_count::<Mat4>(usize::MAX, "ctx").unwrap_err();
        matches!(err, UsdBridgeError::AllocTooLarge { .. });
    }

    #[test]
    fn safe_mul_count_happy() {
        assert_eq!(safe_mul_count::<f32>(1024, 3, "ctx"), Some(3072));
    }

    #[test]
    fn safe_mul_count_overflow_is_none() {
        assert_eq!(safe_mul_count::<f32>(usize::MAX, 3, "ctx"), None);
    }

    #[test]
    fn safe_mul_count_too_large_is_none() {
        // 2B floats * 4B = 8 GiB > 4 GiB cap.
        assert_eq!(safe_mul_count::<f32>(2_000_000_000, 1, "ctx"), None);
    }
}
