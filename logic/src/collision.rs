use cgmath::{prelude::*, Point3, Vector3};

#[derive(Debug, Copy, Clone)]
pub struct AlignedBox {
    /// The leftmost (smallestt) plane on each axis.
    pub low: Point3<f32>,
    /// The rightmost (greatest) plane on each axis.
    pub high: Point3<f32>,
}

#[derive(Debug, Copy, Clone)]
pub struct Overlap {
    /// The volume of the intersection.
    pub volume: f32,
    /// The amount to translate one of the objects to move outside the collision.
    pub resolution: Vector3<f32>,
}

impl AlignedBox {
    /// Create a new bounding box centered around a given point.
    pub fn centered(center: Point3<f32>, size: Vector3<f32>) -> Self {
        AlignedBox {
            low: center - 0.5 * size,
            high: center + 0.5 * size,
        }
    }

    /// Move the bounding box.
    pub fn translate(self, amount: Vector3<f32>) -> Self {
        AlignedBox {
            low: self.low + amount,
            high: self.high + amount,
        }
    }

    /// True iff the given point is within the bounding box or its boundary.
    pub fn contains(self, point: Point3<f32>) -> bool {
        (0..3).all(|i| self.low[i] <= point[i] && point[i] <= self.high[i])
    }

    /// True iff the intersection of the bounding boxes contains points not in their boundaries.
    pub fn intersects(self, other: Self) -> bool {
        (0..3).all(|i| self.low[i] < other.high[i] && other.low[i] < self.high[i])
    }

    /// True iff the intersection of the bounding boxes contains any points, including their boundaries.
    pub fn touches(self, other: Self) -> bool {
        (0..3).all(|i| self.low[i] <= other.high[i] && other.low[i] <= self.high[i])
    }

    /// If possible, find the vector of minimum overlap.
    pub fn overlap(self, other: Self) -> Option<Overlap> {
        if self.intersects(other) {
            Some(self.overlap_unchecked(other))
        } else {
            None
        }
    }

    /// Return the vector of minimum overlap between two intersecting boxes. That is, the minimum
    /// distance to translate the `self` box in order no longer intersect.
    pub fn overlap_unchecked(self, other: Self) -> Overlap {
        let mut min_overlap = std::f32::INFINITY;
        let mut resolution = Vector3::zero();

        let mut compare_and_swap = |distance, axis, direction| {
            if distance < min_overlap {
                min_overlap = distance;
                resolution = Vector3::zero();
                resolution[axis] = direction;
            }
        };

        let mut volume = 1.0;

        for i in 0..3 {
            let left = self.high[i] - other.low[i];
            let right = other.high[i] - self.low[i];

            compare_and_swap(left, i, -left);
            compare_and_swap(right, i, right);

            volume *= f32::min(left, right);
        }

        Overlap { volume, resolution }
    }
}
