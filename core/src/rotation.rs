//! A rotation in 3D, which turns a vector's direction without changing its length.

use std::f32::consts::FRAC_PI_2;

/// A vector as its `x`, `y` and `z`.
type Vector = (f32, f32, f32);

/// A rotation as a unit quaternion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rotation {
    w: f32,
    x: f32,
    y: f32,
    z: f32,
}

impl Rotation {
    pub const IDENTITY: Self = Self {
        w: 1.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    /// A turn of `angle` radians about the unit `axis`, counterclockwise looking down on it.
    pub fn about((x, y, z): Vector, angle: f32) -> Self {
        let (sin, cos) = (angle / 2.0).sin_cos();
        Self {
            w: cos,
            x: x * sin,
            y: y * sin,
            z: z * sin,
        }
    }

    /// The rotation taking `+z` to the direction at `polar` radians from `+z` and `azimuth`
    /// radians counterclockwise from `+x` around it, without a twist about that direction.
    pub fn spherical(polar: f32, azimuth: f32) -> Self {
        Self::about((0.0, 1.0, 0.0), polar).then(Self::about((0.0, 0.0, 1.0), azimuth))
    }

    /// The [`spherical`](Self::spherical) rotation taking `+z` to the unit `direction`, with
    /// `azimuth` on the poles, which have none of their own.
    pub fn toward((x, y, z): Vector, azimuth: f32) -> Self {
        let polar = z.clamp(-1.0, 1.0).acos();
        let azimuth = if x == 0.0 && y == 0.0 {
            azimuth
        } else {
            y.atan2(x)
        };
        Self::spherical(polar, azimuth)
    }

    /// This rotation followed by `next`.
    pub fn then(self, next: Self) -> Self {
        let (a, b) = (next, self);
        let product = Self {
            w: a.w * b.w - a.x * b.x - a.y * b.y - a.z * b.z,
            x: a.w * b.x + a.x * b.w + a.y * b.z - a.z * b.y,
            y: a.w * b.y - a.x * b.z + a.y * b.w + a.z * b.x,
            z: a.w * b.z + a.x * b.y - a.y * b.x + a.z * b.w,
        };
        // Renormalizing keeps a long chain of small turns from drifting into a scale.
        let norm = (product.w * product.w
            + product.x * product.x
            + product.y * product.y
            + product.z * product.z)
            .sqrt();
        Self {
            w: product.w / norm,
            x: product.x / norm,
            y: product.y / norm,
            z: product.z / norm,
        }
    }

    /// `vector` turned by this rotation.
    pub fn apply(self, (vx, vy, vz): Vector) -> Vector {
        let Self { w, x, y, z } = self;
        // v + 2w(q × v) + 2q × (q × v), with q the quaternion's vector part.
        let (tx, ty, tz) = (
            2.0 * (y * vz - z * vy),
            2.0 * (z * vx - x * vz),
            2.0 * (x * vy - y * vx),
        );
        (
            vx + w * tx + (y * tz - z * ty),
            vy + w * ty + (z * tx - x * tz),
            vz + w * tz + (x * ty - y * tx),
        )
    }

    /// The polar and azimuthal angles, in radians, of the direction this rotation takes `+z`
    /// to. On a pole the azimuth is read from where `+x` goes instead, so a [`spherical`]
    /// rotation gives its own back even there.
    ///
    /// [`spherical`]: Self::spherical
    pub fn angles(self) -> (f32, f32) {
        let (x, y, z) = self.apply((0.0, 0.0, 1.0));
        let polar = z.clamp(-1.0, 1.0).acos();
        let azimuth = if x.hypot(y) > 1e-6 {
            y.atan2(x)
        } else {
            // `+x` goes to (cos θ cos φ, cos θ sin φ, -sin θ), and cos θ is ±1 here.
            let (x, y, _) = self.apply((1.0, 0.0, 0.0));
            if polar < FRAC_PI_2 {
                y.atan2(x)
            } else {
                (-y).atan2(-x)
            }
        };
        (polar, azimuth)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn close((ax, ay, az): Vector, (bx, by, bz): Vector) -> bool {
        (ax - bx).abs() < 1e-5 && (ay - by).abs() < 1e-5 && (az - bz).abs() < 1e-5
    }

    #[test]
    fn a_quarter_turn_about_y_takes_z_to_x() {
        let turned = Rotation::about((0.0, 1.0, 0.0), FRAC_PI_2).apply((0.0, 0.0, 1.0));
        assert!(close(turned, (1.0, 0.0, 0.0)), "{turned:?}");
    }

    #[test]
    fn then_applies_the_first_rotation_first() {
        let rotation = Rotation::about((0.0, 1.0, 0.0), FRAC_PI_2)
            .then(Rotation::about((0.0, 0.0, 1.0), FRAC_PI_2));
        let turned = rotation.apply((0.0, 0.0, 1.0));
        assert!(close(turned, (0.0, 1.0, 0.0)), "{turned:?}");
    }

    #[test]
    fn a_spherical_rotation_gives_its_angles_back() {
        for (polar, azimuth) in [(0.3, 1.2), (2.5, -2.0), (0.0, 0.7), (PI, -1.1)] {
            let (got_polar, got_azimuth) = Rotation::spherical(polar, azimuth).angles();
            assert!(
                (got_polar - polar).abs() < 1e-5 && (got_azimuth - azimuth).abs() < 1e-5,
                "{polar}, {azimuth}: {got_polar}, {got_azimuth}"
            );
        }
    }

    #[test]
    fn toward_points_z_along_the_direction() {
        let direction = (0.48, -0.6, 0.64);
        let turned = Rotation::toward(direction, 0.0).apply((0.0, 0.0, 1.0));
        assert!(close(turned, direction), "{turned:?}");
    }

    #[test]
    fn toward_a_pole_keeps_the_given_azimuth() {
        let (_, azimuth) = Rotation::toward((0.0, 0.0, -1.0), 0.9).angles();
        assert!((azimuth - 0.9).abs() < 1e-5, "{azimuth}");
    }
}
