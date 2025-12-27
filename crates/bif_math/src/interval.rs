#[derive(Debug, Clone, Copy, PartialEq)]

pub struct Interval {
    pub min: f32,
    pub max: f32,
}

impl Interval {
    /// Create a new interval given min and max values.
    pub fn new(min: f32, max: f32) -> Self {
        Self { min, max }
    }

    /// Returns the size of the interval (max - min).
    pub fn size(&self) -> f32 {
        self.max - self.min
    }

    /// Returns true if x is within the interval [min, max] (inclusive).
    pub fn contains(&self, x: f32) -> bool {
        self.min <= x && x <= self.max
    }

    /// Returns true if x is strictly within the interval (min, max) (exclusive).
    pub fn surrounds(&self, x: f32) -> bool {
        self.min < x && x < self.max
    }

    /// Clamps x to be within the interval [min, max].
    pub fn clamp(&self, x: f32) -> f32 {
        x.clamp(self.min, self.max)
    }

    /// Expands the interval by delta/2 on each side.
    pub fn expand(&self, delta: f32) -> Interval {
        let padding = delta / 2.0;
        Interval::new(self.min - padding, self.max + padding)
    }

    /// Adds two intervals component-wise (min + min, max + max).
    pub fn add(&self, other: &Interval) -> Interval {
        Interval::new(self.min + other.min, self.max + other.max)
    }

    /// Adds a scalar displacement to both min and max.
    pub fn add_scalar(&self, displacement: f32) -> Interval {
        Interval::new(self.min + displacement, self.max + displacement)
    }

    /// Creates an interval that surrounds two other intervals.
    pub fn surrounding(a: &Interval, b: &Interval) -> Interval {
        Interval::new(a.min.min(b.min), a.max.max(b.max))
    }

    /// An empty interval (min > max, contains nothing).
    pub const EMPTY: Interval = Interval {
        min: f32::INFINITY,
        max: f32::NEG_INFINITY,
    };

    /// A universe interval (contains everything).
    pub const UNIVERSE: Interval = Interval {
        min: f32::NEG_INFINITY,
        max: f32::INFINITY,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interval_creation() {
        let interval = Interval::new(0.0, 10.0);
        assert_eq!(interval.min, 0.0);
        assert_eq!(interval.max, 10.0);
    }

    #[test]
    fn test_interval_size() {
        let interval = Interval::new(2.0, 7.0);
        assert_eq!(interval.size(), 5.0);

        let negative = Interval::new(-5.0, 5.0);
        assert_eq!(negative.size(), 10.0);
    }

    #[test]
    fn test_interval_contains() {
        let interval = Interval::new(0.0, 10.0);

        // Inclusive bounds
        assert!(interval.contains(0.0));
        assert!(interval.contains(10.0));
        assert!(interval.contains(5.0));

        // Outside bounds
        assert!(!interval.contains(-0.1));
        assert!(!interval.contains(10.1));
    }

    #[test]
    fn test_interval_surrounds() {
        let interval = Interval::new(0.0, 10.0);

        // Exclusive bounds - endpoints NOT included
        assert!(!interval.surrounds(0.0));
        assert!(!interval.surrounds(10.0));

        // Inside
        assert!(interval.surrounds(5.0));
        assert!(interval.surrounds(0.1));
        assert!(interval.surrounds(9.9));

        // Outside
        assert!(!interval.surrounds(-0.1));
        assert!(!interval.surrounds(10.1));
    }

    #[test]
    fn test_interval_clamp() {
        let interval = Interval::new(0.0, 10.0);

        assert_eq!(interval.clamp(-5.0), 0.0);
        assert_eq!(interval.clamp(0.0), 0.0);
        assert_eq!(interval.clamp(5.0), 5.0);
        assert_eq!(interval.clamp(10.0), 10.0);
        assert_eq!(interval.clamp(15.0), 10.0);
    }

    #[test]
    fn test_interval_expand() {
        let interval = Interval::new(0.0, 10.0);
        let expanded = interval.expand(4.0);

        // Expanded by 2.0 on each side (4.0 / 2)
        assert_eq!(expanded.min, -2.0);
        assert_eq!(expanded.max, 12.0);
        assert_eq!(expanded.size(), 14.0);
    }

    #[test]
    fn test_interval_add() {
        let a = Interval::new(1.0, 5.0);
        let b = Interval::new(2.0, 3.0);
        let result = a.add(&b);

        assert_eq!(result.min, 3.0);
        assert_eq!(result.max, 8.0);
    }

    #[test]
    fn test_interval_empty() {
        let empty = Interval::EMPTY;

        // Empty interval has min > max
        assert!(empty.min > empty.max);
        assert_eq!(empty.size(), f32::NEG_INFINITY);

        // Contains nothing
        assert!(!empty.contains(0.0));
        assert!(!empty.contains(f32::INFINITY));
    }

    #[test]
    fn test_interval_universe() {
        let universe = Interval::UNIVERSE;

        // Universe interval spans all values
        assert!(universe.contains(0.0));
        assert!(universe.contains(1e10));
        assert!(universe.contains(-1e10));
        assert_eq!(universe.size(), f32::INFINITY);
    }

    #[test]
    fn test_interval_copy() {
        let a = Interval::new(1.0, 5.0);
        let b = a; // Copy, not move

        // Both should be usable
        assert_eq!(a.size(), b.size());
        assert_eq!(a.contains(3.0), b.contains(3.0));
    }
}