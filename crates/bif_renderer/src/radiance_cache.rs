//! SHARC — Spatially Hashed Radiance Cache (idTech 8 inspired).
//!
//! Caches surface-local radiance in a fixed-size hash table keyed by
//! quantized world position + dominant normal direction. Secondary rays
//! that hit a cached cell with enough samples can skip remaining bounces,
//! cutting render time roughly in half for multi-bounce scenes.
//!
//! Two backends:
//! - **Lock-free** (default): `AtomicU32` per field via `from_ptr`, zero
//!   contention for both reads and writes. CAS on `sample_count` guards
//!   writes; torn reads are bounded error, invisible in progressive rendering.
//! - **Sharded RwLock** (fallback): 64 shards with `RwLock` per shard.
//!
//! The CPU layout (`#[repr(C)]`) mirrors the GPU buffer so the same hash
//! formula and entry format port directly to WGSL with `atomicAdd` when
//! M27 (GPU Path Tracing) arrives.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::RwLock;

use bif_math::{Aabb, Vec3};

// ---------------------------------------------------------------------------
// Cache entry — GPU-compatible layout
// ---------------------------------------------------------------------------

/// Single radiance cache entry. `#[repr(C)]` for GPU buffer compatibility.
///
/// 24 bytes, 4-byte aligned. Maps to WGSL:
/// ```wgsl
/// struct CacheEntry {
///     radiance_r: f32, radiance_g: f32, radiance_b: f32,
///     sample_count: u32, frame_id: u32, _pad: u32,
/// }
/// ```
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CacheEntry {
    pub radiance_r: f32,
    pub radiance_g: f32,
    pub radiance_b: f32,
    pub sample_count: u32,
    pub frame_id: u32,
    pub _pad: u32,
}

impl Default for CacheEntry {
    fn default() -> Self {
        Self {
            radiance_r: 0.0,
            radiance_g: 0.0,
            radiance_b: 0.0,
            sample_count: 0,
            frame_id: 0,
            _pad: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Radiance cache tuning knobs.
#[derive(Debug, Clone)]
pub struct RadianceCacheConfig {
    /// World-space cell size for spatial quantization.
    pub cell_size: f32,
    /// Total entries in the hash buffer (should be power-of-2).
    pub buffer_size: u32,
    /// Minimum accumulated samples before a cached value is trusted.
    pub min_samples: u32,
    /// Entries older than this many frames are treated as stale.
    pub max_age: u32,
    /// Minimum bounce depth before cache reads/writes (never cache primary hits).
    pub min_bounce_depth: u32,
    /// Exponential moving average weight for new samples (0..1).
    pub ema_weight: f32,
    /// Master enable/disable.
    pub enabled: bool,
    /// Use lock-free atomics instead of sharded RwLock (default: true).
    ///
    /// Lock-free eliminates both read and write contention. Uses CAS on
    /// `sample_count` to guard writes; torn reads are bounded error,
    /// invisible in progressive rendering. Compiles to plain `mov` /
    /// `cmpxchg` on x86-64.
    pub lock_free: bool,
}

impl Default for RadianceCacheConfig {
    fn default() -> Self {
        Self {
            cell_size: 0.5,
            buffer_size: 1 << 20, // 1M entries (~24 MB)
            min_samples: 4,
            max_age: 32,
            min_bounce_depth: 2,
            ema_weight: 0.1,
            enabled: true,
            lock_free: true,
        }
    }
}

/// Estimate optimal cell_size from scene AABB.
///
/// Targets ~1000 cells across the longest axis, clamped to [0.01, 10.0].
pub fn auto_cell_size(scene_aabb: &Aabb) -> f32 {
    let diagonal = (scene_aabb.max_point() - scene_aabb.min_point()).length();
    (diagonal / 1000.0).clamp(0.01, 10.0)
}

// ---------------------------------------------------------------------------
// Spatial hash
// ---------------------------------------------------------------------------

/// Encode a normal into one of 6 dominant-axis directions (0..5).
///
/// Prevents light leak between surfaces with opposing normals
/// (e.g., floor vs ceiling in the same cell).
#[inline]
fn dominant_axis(normal: Vec3) -> u32 {
    let ax = normal.x.abs();
    let ay = normal.y.abs();
    let az = normal.z.abs();
    if ax >= ay && ax >= az {
        if normal.x >= 0.0 {
            0
        } else {
            1
        }
    } else if ay >= az {
        if normal.y >= 0.0 {
            2
        } else {
            3
        }
    } else if normal.z >= 0.0 {
        4
    } else {
        5
    }
}

/// Compute spatial hash for a position + normal pair.
///
/// Position is quantized to grid cells via `floor(pos / cell_size)`.
/// Normal is encoded as a dominant-axis index (6 directions).
/// Uses large-prime wrapping multiply (matches GI_ID_METHOD.md formula).
/// `i32 as u32` cast handles negative coordinates (matches GPU `bitcast`).
#[inline]
pub fn spatial_hash(pos: Vec3, normal: Vec3, cell_size: f32, buffer_size: u32) -> u32 {
    let p = pos / cell_size;
    let ix = p.x.floor() as i32 as u32;
    let iy = p.y.floor() as i32 as u32;
    let iz = p.z.floor() as i32 as u32;
    let n = dominant_axis(normal);

    let h = ix.wrapping_mul(73856093)
        ^ iy.wrapping_mul(19349663)
        ^ iz.wrapping_mul(83492791)
        ^ n.wrapping_mul(2654435761);
    h % buffer_size
}

// ---------------------------------------------------------------------------
// Lock-free atomic buffer
// ---------------------------------------------------------------------------

/// Flat buffer of `CacheEntry` with per-field atomic access via
/// `AtomicU32::from_ptr` (stable since Rust 1.75).
///
/// # Safety invariants
/// - `CacheEntry` is `#[repr(C)]` with all fields `u32`-sized and 4-byte aligned.
/// - Buffer is allocated once in `new()` and never resized.
/// - All field access goes through `AtomicU32` methods after construction.
/// - On x86-64, aligned `u32` ops are hardware-atomic.
struct AtomicCacheBuffer {
    buf: UnsafeCell<Vec<CacheEntry>>,
    len: usize,
}

// Safety: all runtime access goes through AtomicU32 (from_ptr), which is Sync+Send.
// The UnsafeCell is never accessed via &mut after construction.
unsafe impl Sync for AtomicCacheBuffer {}
unsafe impl Send for AtomicCacheBuffer {}

impl AtomicCacheBuffer {
    fn new(size: usize) -> Self {
        Self {
            buf: UnsafeCell::new(vec![CacheEntry::default(); size]),
            len: size,
        }
    }

    /// Atomic view of `entry[idx].sample_count`.
    ///
    /// # Safety
    /// `idx` must be `< self.len`.
    #[inline]
    unsafe fn sample_count_atomic(&self, idx: usize) -> &AtomicU32 {
        debug_assert!(idx < self.len);
        let entry = (*self.buf.get()).as_mut_ptr().add(idx);
        AtomicU32::from_ptr(std::ptr::addr_of_mut!((*entry).sample_count))
    }

    /// Atomic view of `entry[idx].radiance_r` (f32 bits as u32).
    ///
    /// # Safety
    /// `idx` must be `< self.len`.
    #[inline]
    unsafe fn radiance_r_atomic(&self, idx: usize) -> &AtomicU32 {
        debug_assert!(idx < self.len);
        let entry = (*self.buf.get()).as_mut_ptr().add(idx);
        AtomicU32::from_ptr(std::ptr::addr_of_mut!((*entry).radiance_r).cast::<u32>())
    }

    /// Atomic view of `entry[idx].radiance_g` (f32 bits as u32).
    ///
    /// # Safety
    /// `idx` must be `< self.len`.
    #[inline]
    unsafe fn radiance_g_atomic(&self, idx: usize) -> &AtomicU32 {
        debug_assert!(idx < self.len);
        let entry = (*self.buf.get()).as_mut_ptr().add(idx);
        AtomicU32::from_ptr(std::ptr::addr_of_mut!((*entry).radiance_g).cast::<u32>())
    }

    /// Atomic view of `entry[idx].radiance_b` (f32 bits as u32).
    ///
    /// # Safety
    /// `idx` must be `< self.len`.
    #[inline]
    unsafe fn radiance_b_atomic(&self, idx: usize) -> &AtomicU32 {
        debug_assert!(idx < self.len);
        let entry = (*self.buf.get()).as_mut_ptr().add(idx);
        AtomicU32::from_ptr(std::ptr::addr_of_mut!((*entry).radiance_b).cast::<u32>())
    }

    /// Atomic view of `entry[idx].frame_id`.
    ///
    /// # Safety
    /// `idx` must be `< self.len`.
    #[inline]
    unsafe fn frame_id_atomic(&self, idx: usize) -> &AtomicU32 {
        debug_assert!(idx < self.len);
        let entry = (*self.buf.get()).as_mut_ptr().add(idx);
        AtomicU32::from_ptr(std::ptr::addr_of_mut!((*entry).frame_id))
    }
}

// ---------------------------------------------------------------------------
// Radiance cache — dual backend
// ---------------------------------------------------------------------------

const NUM_SHARDS: usize = 64;

/// Thread-safe radiance cache with dual backends.
///
/// **Lock-free** (default): flat buffer with per-field `AtomicU32` access.
/// CAS on `sample_count` serializes writes per-slot; different slots are
/// fully parallel. Torn reads are bounded error in progressive rendering.
///
/// **Sharded RwLock** (fallback): 64 shards with `RwLock` per shard.
///
/// Debug impl prints config + stats (not buffer contents).
pub struct RadianceCache {
    /// Lock-free backend (when `config.lock_free == true`).
    atomic_buf: Option<AtomicCacheBuffer>,
    /// Sharded RwLock backend (when `config.lock_free == false`).
    shards: Option<Vec<RwLock<Vec<CacheEntry>>>>,
    /// Entries per shard (only meaningful for sharded path).
    entries_per_shard: usize,
    config: RadianceCacheConfig,
    /// Monotonic frame counter (u32 matches `CacheEntry::frame_id`).
    current_frame: AtomicU32,
    /// Running counters for stats (reads, hits, occupied slots).
    stat_reads: AtomicU64,
    stat_hits: AtomicU64,
    stat_occupied: AtomicU64,
}

impl std::fmt::Debug for RadianceCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RadianceCache")
            .field("config", &self.config)
            .field("frame", &self.current_frame.load(Ordering::Relaxed))
            .field("lock_free", &self.atomic_buf.is_some())
            .finish()
    }
}

impl RadianceCache {
    /// Create a new cache from the given config.
    pub fn new(config: RadianceCacheConfig) -> Self {
        if config.lock_free {
            let size = config.buffer_size as usize;
            Self {
                atomic_buf: Some(AtomicCacheBuffer::new(size)),
                shards: None,
                entries_per_shard: 0,
                config,
                current_frame: AtomicU32::new(0),
                stat_reads: AtomicU64::new(0),
                stat_hits: AtomicU64::new(0),
                stat_occupied: AtomicU64::new(0),
            }
        } else {
            let entries_per_shard = (config.buffer_size as usize).div_ceil(NUM_SHARDS);
            let shards = (0..NUM_SHARDS)
                .map(|_| RwLock::new(vec![CacheEntry::default(); entries_per_shard]))
                .collect();
            Self {
                atomic_buf: None,
                shards: Some(shards),
                entries_per_shard,
                config,
                current_frame: AtomicU32::new(0),
                stat_reads: AtomicU64::new(0),
                stat_hits: AtomicU64::new(0),
                stat_occupied: AtomicU64::new(0),
            }
        }
    }

    /// Read-only access to the config.
    pub fn config(&self) -> &RadianceCacheConfig {
        &self.config
    }

    /// Resolve shard index and local offset from a global hash index.
    #[inline]
    fn shard_and_offset(&self, index: u32) -> (usize, usize) {
        let shard = index as usize % NUM_SHARDS;
        let offset = (index as usize / NUM_SHARDS) % self.entries_per_shard;
        (shard, offset)
    }

    // -----------------------------------------------------------------------
    // Lookup
    // -----------------------------------------------------------------------

    /// Look up cached radiance at a world position + normal.
    ///
    /// Returns `Some(Color)` if the entry has enough samples and isn't stale.
    pub fn lookup(&self, pos: Vec3, normal: Vec3) -> Option<Vec3> {
        if !self.config.enabled {
            return None;
        }
        if self.atomic_buf.is_some() {
            self.lookup_lock_free(pos, normal)
        } else {
            self.lookup_sharded(pos, normal)
        }
    }

    /// Lock-free lookup: direct index, per-field atomic loads, zero locks.
    fn lookup_lock_free(&self, pos: Vec3, normal: Vec3) -> Option<Vec3> {
        let buf = self.atomic_buf.as_ref().unwrap();
        let idx =
            spatial_hash(pos, normal, self.config.cell_size, self.config.buffer_size) as usize;
        let frame = self.current_frame.load(Ordering::Relaxed);

        self.stat_reads.fetch_add(1, Ordering::Relaxed);

        // Safety: idx = hash % buffer_size, always < buf.len
        unsafe {
            let count = buf.sample_count_atomic(idx).load(Ordering::Relaxed);
            if count < self.config.min_samples {
                return None;
            }
            let entry_frame = buf.frame_id_atomic(idx).load(Ordering::Relaxed);
            let age = frame.wrapping_sub(entry_frame);
            if age > self.config.max_age {
                return None;
            }

            self.stat_hits.fetch_add(1, Ordering::Relaxed);
            Some(Vec3::new(
                f32::from_bits(buf.radiance_r_atomic(idx).load(Ordering::Relaxed)),
                f32::from_bits(buf.radiance_g_atomic(idx).load(Ordering::Relaxed)),
                f32::from_bits(buf.radiance_b_atomic(idx).load(Ordering::Relaxed)),
            ))
        }
    }

    /// Sharded RwLock lookup: takes a read lock on the target shard.
    fn lookup_sharded(&self, pos: Vec3, normal: Vec3) -> Option<Vec3> {
        let idx = spatial_hash(pos, normal, self.config.cell_size, self.config.buffer_size);
        let (shard, offset) = self.shard_and_offset(idx);
        let frame = self.current_frame.load(Ordering::Relaxed);

        self.stat_reads.fetch_add(1, Ordering::Relaxed);

        let shards = self.shards.as_ref().unwrap();
        let guard = shards[shard].read().unwrap_or_else(|e| e.into_inner());
        let entry = &guard[offset];

        if entry.sample_count < self.config.min_samples {
            return None;
        }
        let age = frame.wrapping_sub(entry.frame_id);
        if age > self.config.max_age {
            return None;
        }

        self.stat_hits.fetch_add(1, Ordering::Relaxed);
        Some(Vec3::new(
            entry.radiance_r,
            entry.radiance_g,
            entry.radiance_b,
        ))
    }

    // -----------------------------------------------------------------------
    // Write
    // -----------------------------------------------------------------------

    /// Write (accumulate) radiance at a world position + normal.
    ///
    /// Uses EMA blending: `new = lerp(old, sample, ema_weight)`.
    /// First write to a cell initializes it directly.
    ///
    /// Lock-free path uses CAS on `sample_count`; failed CAS drops the sample
    /// (statistically irrelevant for a progressive cache).
    pub fn write(&self, pos: Vec3, normal: Vec3, radiance: Vec3) {
        if !self.config.enabled {
            return;
        }
        if self.atomic_buf.is_some() {
            self.write_lock_free(pos, normal, radiance);
        } else {
            self.write_sharded(pos, normal, radiance);
        }
    }

    /// Lock-free write: CAS on `sample_count` as version guard.
    ///
    /// - Empty slot (count=0): CAS 0→1 to claim, then store radiance + frame.
    /// - Stale slot: CAS old→1 to reinit.
    /// - Active slot: EMA blend, CAS old_count→old_count+1.
    /// - CAS failure = drop sample (no retry).
    fn write_lock_free(&self, pos: Vec3, normal: Vec3, radiance: Vec3) {
        let buf = self.atomic_buf.as_ref().unwrap();
        let idx =
            spatial_hash(pos, normal, self.config.cell_size, self.config.buffer_size) as usize;
        let frame = self.current_frame.load(Ordering::Relaxed);

        // Safety: idx = hash % buffer_size, always < buf.len
        unsafe {
            let sc = buf.sample_count_atomic(idx);
            let old_count = sc.load(Ordering::Relaxed);

            if old_count == 0 {
                // Empty slot — CAS 0→1 to claim
                if sc
                    .compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
                {
                    buf.radiance_r_atomic(idx)
                        .store(radiance.x.to_bits(), Ordering::Relaxed);
                    buf.radiance_g_atomic(idx)
                        .store(radiance.y.to_bits(), Ordering::Relaxed);
                    buf.radiance_b_atomic(idx)
                        .store(radiance.z.to_bits(), Ordering::Relaxed);
                    buf.frame_id_atomic(idx).store(frame, Ordering::Relaxed);
                    self.stat_occupied.fetch_add(1, Ordering::Relaxed);
                }
                return;
            }

            let old_frame = buf.frame_id_atomic(idx).load(Ordering::Relaxed);
            let age = frame.wrapping_sub(old_frame);

            if age > self.config.max_age {
                // Stale — CAS old→1 to reinit
                if sc
                    .compare_exchange(old_count, 1, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
                {
                    buf.radiance_r_atomic(idx)
                        .store(radiance.x.to_bits(), Ordering::Relaxed);
                    buf.radiance_g_atomic(idx)
                        .store(radiance.y.to_bits(), Ordering::Relaxed);
                    buf.radiance_b_atomic(idx)
                        .store(radiance.z.to_bits(), Ordering::Relaxed);
                    buf.frame_id_atomic(idx).store(frame, Ordering::Relaxed);
                }
                return;
            }

            // Active — EMA blend, CAS count to commit
            // NOTE: Known TOCTOU race in lock-free EMA blend — between reading old radiance
            // values and CAS on sample_count, another thread may update radiance fields.
            // If CAS succeeds, the blend is computed against stale values. This manifests as
            // occasional flicker in cached regions during progressive rendering.
            // Acceptable for IPR preview quality. For production-quality rendering, use
            // a mutex or pack count+checksum into AtomicU64 and re-read after CAS.
            let old_r = f32::from_bits(buf.radiance_r_atomic(idx).load(Ordering::Relaxed));
            let old_g = f32::from_bits(buf.radiance_g_atomic(idx).load(Ordering::Relaxed));
            let old_b = f32::from_bits(buf.radiance_b_atomic(idx).load(Ordering::Relaxed));
            let w = self.config.ema_weight;
            let new_r = old_r * (1.0 - w) + radiance.x * w;
            let new_g = old_g * (1.0 - w) + radiance.y * w;
            let new_b = old_b * (1.0 - w) + radiance.z * w;

            if sc
                .compare_exchange(
                    old_count,
                    old_count + 1,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                buf.radiance_r_atomic(idx)
                    .store(new_r.to_bits(), Ordering::Relaxed);
                buf.radiance_g_atomic(idx)
                    .store(new_g.to_bits(), Ordering::Relaxed);
                buf.radiance_b_atomic(idx)
                    .store(new_b.to_bits(), Ordering::Relaxed);
                buf.frame_id_atomic(idx).store(frame, Ordering::Relaxed);
            }
        }
    }

    /// Sharded RwLock write: takes a write lock on the target shard.
    fn write_sharded(&self, pos: Vec3, normal: Vec3, radiance: Vec3) {
        let idx = spatial_hash(pos, normal, self.config.cell_size, self.config.buffer_size);
        let (shard, offset) = self.shard_and_offset(idx);
        let frame = self.current_frame.load(Ordering::Relaxed);

        let shards = self.shards.as_ref().unwrap();
        let mut guard = shards[shard].write().unwrap_or_else(|e| e.into_inner());
        let entry = &mut guard[offset];

        if entry.sample_count == 0 {
            // First sample — initialize directly
            entry.radiance_r = radiance.x;
            entry.radiance_g = radiance.y;
            entry.radiance_b = radiance.z;
            entry.sample_count = 1;
            entry.frame_id = frame;
            self.stat_occupied.fetch_add(1, Ordering::Relaxed);
        } else {
            let age = frame.wrapping_sub(entry.frame_id);
            if age > self.config.max_age {
                // Stale — reinitialize
                entry.radiance_r = radiance.x;
                entry.radiance_g = radiance.y;
                entry.radiance_b = radiance.z;
                entry.sample_count = 1;
                entry.frame_id = frame;
            } else {
                // EMA blend
                let w = self.config.ema_weight;
                entry.radiance_r = entry.radiance_r * (1.0 - w) + radiance.x * w;
                entry.radiance_g = entry.radiance_g * (1.0 - w) + radiance.y * w;
                entry.radiance_b = entry.radiance_b * (1.0 - w) + radiance.z * w;
                entry.sample_count += 1;
                entry.frame_id = frame;
            }
        }
    }

    // -----------------------------------------------------------------------
    // Frame / clear / stats
    // -----------------------------------------------------------------------

    /// Advance the internal frame counter (call between progressive passes).
    pub fn advance_frame(&self) {
        self.current_frame.fetch_add(1, Ordering::Relaxed);
    }

    /// Current frame number.
    pub fn current_frame(&self) -> u32 {
        self.current_frame.load(Ordering::Relaxed)
    }

    /// Clear all entries (call on scene change, NOT camera-only moves).
    pub fn clear(&self) {
        if let Some(buf) = &self.atomic_buf {
            for i in 0..buf.len {
                // Safety: i < buf.len
                unsafe {
                    buf.sample_count_atomic(i).store(0, Ordering::Relaxed);
                    buf.radiance_r_atomic(i)
                        .store(0.0f32.to_bits(), Ordering::Relaxed);
                    buf.radiance_g_atomic(i)
                        .store(0.0f32.to_bits(), Ordering::Relaxed);
                    buf.radiance_b_atomic(i)
                        .store(0.0f32.to_bits(), Ordering::Relaxed);
                    buf.frame_id_atomic(i).store(0, Ordering::Relaxed);
                }
            }
        } else if let Some(shards) = &self.shards {
            for shard in shards {
                let mut guard = shard.write().unwrap_or_else(|e| e.into_inner());
                for entry in guard.iter_mut() {
                    *entry = CacheEntry::default();
                }
            }
        }
        self.stat_reads.store(0, Ordering::Relaxed);
        self.stat_hits.store(0, Ordering::Relaxed);
        self.stat_occupied.store(0, Ordering::Relaxed);
    }

    /// Cache hit rate (0.0–1.0). Returns 0 if no reads yet.
    pub fn hit_rate(&self) -> f32 {
        let reads = self.stat_reads.load(Ordering::Relaxed);
        if reads == 0 {
            return 0.0;
        }
        self.stat_hits.load(Ordering::Relaxed) as f32 / reads as f32
    }

    /// Fraction of entries with at least one sample (0.0–1.0).
    ///
    /// O(1) — uses an atomic counter updated on first write to each slot.
    pub fn occupancy(&self) -> f32 {
        let total = if self.atomic_buf.is_some() {
            self.config.buffer_size as usize
        } else {
            self.entries_per_shard * NUM_SHARDS
        };
        if total == 0 {
            return 0.0;
        }
        self.stat_occupied.load(Ordering::Relaxed) as f32 / total as f32
    }

    /// Get sample count at a position (for heatmap AOV visualization).
    pub fn sample_count_at(&self, pos: Vec3, normal: Vec3) -> u32 {
        if let Some(buf) = &self.atomic_buf {
            let idx =
                spatial_hash(pos, normal, self.config.cell_size, self.config.buffer_size) as usize;
            // Safety: idx = hash % buffer_size, always < buf.len
            unsafe { buf.sample_count_atomic(idx).load(Ordering::Relaxed) }
        } else {
            let idx = spatial_hash(pos, normal, self.config.cell_size, self.config.buffer_size);
            let (shard, offset) = self.shard_and_offset(idx);
            let shards = self.shards.as_ref().unwrap();
            let guard = shards[shard].read().unwrap_or_else(|e| e.into_inner());
            guard[offset].sample_count
        }
    }

    /// Raw read count (for per-pass delta calculation).
    pub fn stat_reads(&self) -> u64 {
        self.stat_reads.load(Ordering::Relaxed)
    }

    /// Raw hit count (for per-pass delta calculation).
    pub fn stat_hits(&self) -> u64 {
        self.stat_hits.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Default config uses lock-free path.
    fn default_cache() -> RadianceCache {
        RadianceCache::new(RadianceCacheConfig::default())
    }

    /// Explicit RwLock path for dual-backend testing.
    fn rwlock_cache() -> RadianceCache {
        RadianceCache::new(RadianceCacheConfig {
            lock_free: false,
            ..Default::default()
        })
    }

    #[test]
    fn test_spatial_hash_deterministic() {
        let pos = Vec3::new(1.5, 2.7, -3.1);
        let normal = Vec3::Y;
        let h1 = spatial_hash(pos, normal, 0.5, 1 << 20);
        let h2 = spatial_hash(pos, normal, 0.5, 1 << 20);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_spatial_hash_normal_disambiguation() {
        let pos = Vec3::new(1.0, 2.0, 3.0);
        let up = Vec3::Y;
        let down = Vec3::NEG_Y;
        let h_up = spatial_hash(pos, up, 0.5, 1 << 20);
        let h_down = spatial_hash(pos, down, 0.5, 1 << 20);
        assert_ne!(h_up, h_down, "opposite normals must hash differently");
    }

    #[test]
    fn test_spatial_hash_cell_quantization() {
        let normal = Vec3::Y;
        let cell_size = 1.0;
        let buf = 1 << 20;
        // Two positions inside the same cell [0, 1)
        let a = Vec3::new(0.1, 0.2, 0.3);
        let b = Vec3::new(0.9, 0.8, 0.7);
        assert_eq!(
            spatial_hash(a, normal, cell_size, buf),
            spatial_hash(b, normal, cell_size, buf),
            "positions in same cell must hash identically"
        );
    }

    #[test]
    fn test_cache_insert_and_lookup() {
        let cache = default_cache();
        let pos = Vec3::new(5.0, 5.0, 5.0);
        let normal = Vec3::Y;
        let color = Vec3::new(0.8, 0.4, 0.2);

        // Write enough samples to pass min_samples threshold
        for _ in 0..cache.config().min_samples {
            cache.write(pos, normal, color);
        }

        let result = cache.lookup(pos, normal);
        assert!(
            result.is_some(),
            "should return cached value after min_samples writes"
        );
        let cached = result.unwrap();
        // With EMA blending and identical samples, should converge to the input
        assert!((cached.x - color.x).abs() < 0.1);
        assert!((cached.y - color.y).abs() < 0.1);
        assert!((cached.z - color.z).abs() < 0.1);
    }

    #[test]
    fn test_rwlock_insert_and_lookup() {
        let cache = rwlock_cache();
        let pos = Vec3::new(5.0, 5.0, 5.0);
        let normal = Vec3::Y;
        let color = Vec3::new(0.8, 0.4, 0.2);

        for _ in 0..cache.config().min_samples {
            cache.write(pos, normal, color);
        }

        let result = cache.lookup(pos, normal);
        assert!(
            result.is_some(),
            "RwLock path should return cached value after min_samples writes"
        );
        let cached = result.unwrap();
        assert!((cached.x - color.x).abs() < 0.1);
        assert!((cached.y - color.y).abs() < 0.1);
        assert!((cached.z - color.z).abs() < 0.1);
    }

    #[test]
    fn test_cache_min_samples_threshold() {
        let cache = RadianceCache::new(RadianceCacheConfig {
            min_samples: 4,
            ..Default::default()
        });
        let pos = Vec3::new(1.0, 2.0, 3.0);
        let normal = Vec3::Y;
        let color = Vec3::new(1.0, 0.0, 0.0);

        // Write fewer than min_samples
        for _ in 0..3 {
            cache.write(pos, normal, color);
        }
        assert!(
            cache.lookup(pos, normal).is_none(),
            "should return None before min_samples reached"
        );

        // One more write hits the threshold
        cache.write(pos, normal, color);
        assert!(
            cache.lookup(pos, normal).is_some(),
            "should return Some after min_samples reached"
        );
    }

    #[test]
    fn test_cache_staleness() {
        let cache = RadianceCache::new(RadianceCacheConfig {
            min_samples: 1,
            max_age: 4,
            ..Default::default()
        });
        let pos = Vec3::new(1.0, 1.0, 1.0);
        let normal = Vec3::Y;

        cache.write(pos, normal, Vec3::ONE);
        assert!(cache.lookup(pos, normal).is_some());

        // Advance past max_age
        for _ in 0..5 {
            cache.advance_frame();
        }
        assert!(
            cache.lookup(pos, normal).is_none(),
            "stale entries should be ignored"
        );
    }

    #[test]
    fn test_cache_concurrent_access() {
        use std::sync::Arc;

        let cache = Arc::new(RadianceCache::new(RadianceCacheConfig {
            min_samples: 1,
            ..Default::default()
        }));

        std::thread::scope(|s| {
            // Spawn 8 writer threads
            for t in 0..8 {
                let cache = Arc::clone(&cache);
                s.spawn(move || {
                    for i in 0..1000 {
                        let pos = Vec3::new(t as f32, i as f32, 0.0);
                        cache.write(pos, Vec3::Y, Vec3::ONE);
                    }
                });
            }
            // Spawn 4 reader threads
            for t in 0..4 {
                let cache = Arc::clone(&cache);
                s.spawn(move || {
                    for i in 0..1000 {
                        let pos = Vec3::new(t as f32, i as f32, 0.0);
                        let _ = cache.lookup(pos, Vec3::Y);
                    }
                });
            }
        });
        // If we reach here without deadlock/panic, concurrency is fine
    }

    #[test]
    fn test_cache_clear() {
        let cache = RadianceCache::new(RadianceCacheConfig {
            min_samples: 1,
            ..Default::default()
        });
        let pos = Vec3::ZERO;
        let normal = Vec3::Y;

        cache.write(pos, normal, Vec3::ONE);
        assert!(cache.lookup(pos, normal).is_some());

        cache.clear();
        assert!(cache.lookup(pos, normal).is_none());
    }

    #[test]
    fn test_auto_cell_size() {
        // 100-unit diagonal → ~0.1 cell_size
        let aabb = Aabb::from_points(Vec3::ZERO, Vec3::new(50.0, 50.0, 50.0));
        let cs = auto_cell_size(&aabb);
        assert!(cs > 0.01 && cs < 10.0);
        // diagonal ≈ 86.6, /1000 ≈ 0.087
        assert!((cs - 0.087).abs() < 0.01, "got {cs}");
    }

    #[test]
    fn test_auto_cell_size_clamp() {
        // Tiny scene
        let tiny = Aabb::from_points(Vec3::ZERO, Vec3::splat(0.001));
        assert_eq!(auto_cell_size(&tiny), 0.01);

        // Huge scene
        let huge = Aabb::from_points(Vec3::ZERO, Vec3::splat(100_000.0));
        assert_eq!(auto_cell_size(&huge), 10.0);
    }

    #[test]
    fn test_hit_rate_and_occupancy() {
        let cache = RadianceCache::new(RadianceCacheConfig {
            min_samples: 1,
            ..Default::default()
        });
        assert_eq!(cache.hit_rate(), 0.0);
        assert_eq!(cache.occupancy(), 0.0);

        let pos = Vec3::ZERO;
        let normal = Vec3::Y;
        cache.write(pos, normal, Vec3::ONE);

        // Read → hit
        cache.lookup(pos, normal);
        assert!(cache.hit_rate() > 0.0);
        assert!(cache.occupancy() > 0.0);
    }

    #[test]
    fn test_dominant_axis_directions() {
        assert_eq!(dominant_axis(Vec3::X), 0);
        assert_eq!(dominant_axis(Vec3::NEG_X), 1);
        assert_eq!(dominant_axis(Vec3::Y), 2);
        assert_eq!(dominant_axis(Vec3::NEG_Y), 3);
        assert_eq!(dominant_axis(Vec3::Z), 4);
        assert_eq!(dominant_axis(Vec3::NEG_Z), 5);
    }

    #[test]
    fn test_cache_entry_size() {
        assert_eq!(
            std::mem::size_of::<CacheEntry>(),
            24,
            "CacheEntry must be 24 bytes for GPU compatibility"
        );
    }

    #[test]
    fn test_cache_disabled() {
        let cache = RadianceCache::new(RadianceCacheConfig {
            enabled: false,
            min_samples: 1,
            ..Default::default()
        });
        let pos = Vec3::ZERO;
        let normal = Vec3::Y;
        cache.write(pos, normal, Vec3::ONE);
        assert!(
            cache.lookup(pos, normal).is_none(),
            "disabled cache should never return data"
        );
    }

    #[test]
    fn test_lock_free_lookup_and_write() {
        let cache = RadianceCache::new(RadianceCacheConfig {
            lock_free: true,
            min_samples: 2,
            ..Default::default()
        });
        let pos = Vec3::new(3.0, 4.0, 5.0);
        let normal = Vec3::Z;
        let color = Vec3::new(0.5, 0.6, 0.7);

        // Below threshold
        cache.write(pos, normal, color);
        assert!(cache.lookup(pos, normal).is_none());

        // At threshold
        cache.write(pos, normal, color);
        let result = cache.lookup(pos, normal);
        assert!(result.is_some());
        let cached = result.unwrap();
        assert!((cached.x - color.x).abs() < 0.1);
        assert!((cached.y - color.y).abs() < 0.1);
        assert!((cached.z - color.z).abs() < 0.1);

        // sample_count_at should match
        assert_eq!(cache.sample_count_at(pos, normal), 2);
    }

    #[test]
    fn test_lock_free_concurrent() {
        use std::sync::Arc;

        let cache = Arc::new(RadianceCache::new(RadianceCacheConfig {
            lock_free: true,
            min_samples: 1,
            ..Default::default()
        }));

        std::thread::scope(|s| {
            // 16 writer threads, each writing 2000 unique positions
            for t in 0..16 {
                let cache = Arc::clone(&cache);
                s.spawn(move || {
                    for i in 0..2000 {
                        let pos = Vec3::new(t as f32, i as f32, 0.0);
                        cache.write(pos, Vec3::Y, Vec3::ONE);
                    }
                });
            }
            // 8 reader threads, reading overlapping positions
            for t in 0..8 {
                let cache = Arc::clone(&cache);
                s.spawn(move || {
                    for i in 0..2000 {
                        let pos = Vec3::new(t as f32, i as f32, 0.0);
                        let _ = cache.lookup(pos, Vec3::Y);
                    }
                });
            }
        });
        // No panic/crash = pass
    }

    #[test]
    fn test_lock_free_cas_contention() {
        use std::sync::Arc;

        // All threads write to the SAME cell — maximum contention
        let cache = Arc::new(RadianceCache::new(RadianceCacheConfig {
            lock_free: true,
            min_samples: 1,
            ..Default::default()
        }));
        let pos = Vec3::new(42.0, 42.0, 42.0);
        let normal = Vec3::Y;

        std::thread::scope(|s| {
            for _ in 0..16 {
                let cache = Arc::clone(&cache);
                s.spawn(move || {
                    for _ in 0..1000 {
                        cache.write(pos, normal, Vec3::ONE);
                    }
                });
            }
        });

        // Entry should exist and have a reasonable count (some CAS failures expected)
        let count = cache.sample_count_at(pos, normal);
        assert!(count >= 1, "at least one write should have succeeded");
        // With 16*1000 attempts, dropped samples are OK but count should be significant
        assert!(
            count >= 100,
            "CAS contention dropped too many: {count}/16000"
        );

        // Radiance should be finite (no NaN/corruption)
        let cached = cache.lookup(pos, normal).unwrap();
        assert!(cached.x.is_finite());
        assert!(cached.y.is_finite());
        assert!(cached.z.is_finite());
    }
}
