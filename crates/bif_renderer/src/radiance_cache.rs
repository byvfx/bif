//! SHARC — Spatially Hashed Radiance Cache (idTech 8 inspired).
//!
//! Caches surface-local radiance in a fixed-size hash table keyed by
//! quantized world position + dominant normal direction. Secondary rays
//! that hit a cached cell with enough samples can skip remaining bounces,
//! cutting render time roughly in half for multi-bounce scenes.
//!
//! The CPU layout (`#[repr(C)]`) mirrors the GPU buffer so the same hash
//! formula and entry format port directly to WGSL with `atomicAdd` when
//! M27 (GPU Path Tracing) arrives.

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
// Sharded radiance cache
// ---------------------------------------------------------------------------

const NUM_SHARDS: usize = 64;

/// Thread-safe radiance cache with sharded locking.
///
/// Each shard owns a contiguous slice of the flat entry buffer and is
/// protected by its own `RwLock`, so rayon worker threads rarely contend.
///
/// Debug impl prints config + stats (not all shard contents).
pub struct RadianceCache {
    shards: Vec<RwLock<Vec<CacheEntry>>>,
    /// Entries per shard — immutable after construction (avoids locking shard 0).
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
            .field("shards", &self.shards.len())
            .field("entries_per_shard", &self.entries_per_shard)
            .finish()
    }
}

impl RadianceCache {
    /// Create a new cache from the given config.
    pub fn new(config: RadianceCacheConfig) -> Self {
        let entries_per_shard = (config.buffer_size as usize).div_ceil(NUM_SHARDS);
        let shards = (0..NUM_SHARDS)
            .map(|_| RwLock::new(vec![CacheEntry::default(); entries_per_shard]))
            .collect();
        Self {
            shards,
            entries_per_shard,
            config,
            current_frame: AtomicU32::new(0),
            stat_reads: AtomicU64::new(0),
            stat_hits: AtomicU64::new(0),
            stat_occupied: AtomicU64::new(0),
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

    /// Look up cached radiance at a world position + normal.
    ///
    /// Returns `Some(Color)` if the entry has enough samples and isn't stale.
    pub fn lookup(&self, pos: Vec3, normal: Vec3) -> Option<Vec3> {
        if !self.config.enabled {
            return None;
        }
        let idx = spatial_hash(pos, normal, self.config.cell_size, self.config.buffer_size);
        let (shard, offset) = self.shard_and_offset(idx);
        let frame = self.current_frame.load(Ordering::Relaxed);

        self.stat_reads.fetch_add(1, Ordering::Relaxed);

        let guard = self.shards[shard].read().unwrap_or_else(|e| e.into_inner());
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

    /// Write (accumulate) radiance at a world position + normal.
    ///
    /// Uses EMA blending: `new = lerp(old, sample, ema_weight)`.
    /// First write to a cell initializes it directly.
    pub fn write(&self, pos: Vec3, normal: Vec3, radiance: Vec3) {
        if !self.config.enabled {
            return;
        }
        let idx = spatial_hash(pos, normal, self.config.cell_size, self.config.buffer_size);
        let (shard, offset) = self.shard_and_offset(idx);
        let frame = self.current_frame.load(Ordering::Relaxed);

        let mut guard = self.shards[shard]
            .write()
            .unwrap_or_else(|e| e.into_inner());
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
        for shard in &self.shards {
            let mut guard = shard.write().unwrap_or_else(|e| e.into_inner());
            for entry in guard.iter_mut() {
                *entry = CacheEntry::default();
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
        let total = self.entries_per_shard * NUM_SHARDS;
        if total == 0 {
            return 0.0;
        }
        self.stat_occupied.load(Ordering::Relaxed) as f32 / total as f32
    }

    /// Get sample count at a position (for heatmap AOV visualization).
    pub fn sample_count_at(&self, pos: Vec3, normal: Vec3) -> u32 {
        let idx = spatial_hash(pos, normal, self.config.cell_size, self.config.buffer_size);
        let (shard, offset) = self.shard_and_offset(idx);
        let guard = self.shards[shard].read().unwrap_or_else(|e| e.into_inner());
        guard[offset].sample_count
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn default_cache() -> RadianceCache {
        RadianceCache::new(RadianceCacheConfig::default())
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
}
