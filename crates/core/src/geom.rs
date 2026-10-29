use serde::{Deserialize, Serialize};
use std::ops::{Add, Mul, Neg, Sub};

/// A point or vector in millimetres.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    pub fn len(self) -> f64 {
        self.x.hypot(self.y)
    }
    pub fn dist(self, o: Point) -> f64 {
        (self - o).len()
    }
    pub fn lerp(self, o: Point, t: f64) -> Point {
        self + (o - self) * t
    }
    pub fn dot(self, o: Point) -> f64 {
        self.x * o.x + self.y * o.y
    }
    /// Unit vector, or `None` for a (near-)zero vector.
    pub fn unit(self) -> Option<Point> {
        let l = self.len();
        if l > 1e-12 {
            Some(self * (1.0 / l))
        } else {
            None
        }
    }
    /// Vector rotated 90 degrees (clockwise on screen, y down).
    pub fn perp(self) -> Point {
        Point::new(-self.y, self.x)
    }
    pub fn rotate(self, rad: f64) -> Point {
        let (s, c) = rad.sin_cos();
        Point::new(self.x * c - self.y * s, self.x * s + self.y * c)
    }
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

impl Add for Point {
    type Output = Point;
    fn add(self, o: Point) -> Point {
        Point::new(self.x + o.x, self.y + o.y)
    }
}
impl Sub for Point {
    type Output = Point;
    fn sub(self, o: Point) -> Point {
        Point::new(self.x - o.x, self.y - o.y)
    }
}
impl Mul<f64> for Point {
    type Output = Point;
    fn mul(self, k: f64) -> Point {
        Point::new(self.x * k, self.y * k)
    }
}
impl Neg for Point {
    type Output = Point;
    fn neg(self) -> Point {
        Point::new(-self.x, -self.y)
    }
}
