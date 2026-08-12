//! The 2-D affine transform PDF uses for both the graphics state (`cm`) and
//! the text matrix (`Tm`), written `[a b c d e f]`.

use pdf_document::Rect;

/// `[a b c d e f]` — maps `(x, y)` to `(a·x + c·y + e, b·x + d·y + f)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Matrix {
    pub const IDENTITY: Matrix = Matrix {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub fn new(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Self {
        Matrix { a, b, c, d, e, f }
    }

    pub fn translate(tx: f64, ty: f64) -> Self {
        Matrix::new(1.0, 0.0, 0.0, 1.0, tx, ty)
    }

    /// `self` applied first, then `other` — the order `cm` composes in
    /// (`CTM' = cm × CTM`).
    pub fn then(self, other: Matrix) -> Self {
        Matrix {
            a: self.a * other.a + self.b * other.c,
            b: self.a * other.b + self.b * other.d,
            c: self.c * other.a + self.d * other.c,
            d: self.c * other.b + self.d * other.d,
            e: self.e * other.a + self.f * other.c + other.e,
            f: self.e * other.b + self.f * other.d + other.f,
        }
    }

    pub fn apply(self, x: f64, y: f64) -> (f64, f64) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    /// The inverse transform, or `None` when this matrix is singular — a
    /// degenerate `cm` (zero scale) that collapses the plane and cannot be
    /// undone.
    pub fn invert(self) -> Option<Matrix> {
        let determinant = self.a * self.d - self.b * self.c;
        if determinant.abs() < 1e-12 {
            return None;
        }
        let inverse_determinant = 1.0 / determinant;
        Some(Matrix {
            a: self.d * inverse_determinant,
            b: -self.b * inverse_determinant,
            c: -self.c * inverse_determinant,
            d: self.a * inverse_determinant,
            e: (self.c * self.f - self.d * self.e) * inverse_determinant,
            f: (self.b * self.e - self.a * self.f) * inverse_determinant,
        })
    }

    /// The axis-aligned box covering the rectangle `(x, y, width, height)`
    /// after this transform.
    ///
    /// All four corners are mapped, not just two: under a rotating or
    /// skewing matrix the transformed shape is not axis-aligned, and taking
    /// only opposite corners would report a box that misses part of it.
    pub fn bounding_box(self, x: f64, y: f64, width: f64, height: f64) -> Rect {
        let corners = [
            self.apply(x, y),
            self.apply(x + width, y),
            self.apply(x, y + height),
            self.apply(x + width, y + height),
        ];
        let min_x = corners
            .iter()
            .map(|(x, _)| *x)
            .fold(f64::INFINITY, f64::min);
        let max_x = corners
            .iter()
            .map(|(x, _)| *x)
            .fold(f64::NEG_INFINITY, f64::max);
        let min_y = corners
            .iter()
            .map(|(_, y)| *y)
            .fold(f64::INFINITY, f64::min);
        let max_y = corners
            .iter()
            .map(|(_, y)| *y)
            .fold(f64::NEG_INFINITY, f64::max);

        Rect {
            x: min_x,
            y: min_y,
            width: max_x - min_x,
            height: max_y - min_y,
        }
    }

    /// The matrix that maps the unit square onto `rect` — what an image
    /// XObject needs as its placement, since `Do` always paints into the
    /// unit square.
    pub fn placing_unit_square(rect: Rect) -> Self {
        Matrix::new(rect.width, 0.0, 0.0, rect.height, rect.x, rect.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(left: f64, right: f64) -> bool {
        (left - right).abs() < 1e-9
    }

    #[test]
    fn identity_leaves_a_point_alone() {
        assert_eq!(Matrix::IDENTITY.apply(3.0, 4.0), (3.0, 4.0));
    }

    #[test]
    fn composition_applies_the_left_matrix_first() {
        // Scale by 2, then shift right by 10: (1,0) -> (2,0) -> (12,0).
        let scale = Matrix::new(2.0, 0.0, 0.0, 2.0, 0.0, 0.0);
        let shift = Matrix::translate(10.0, 0.0);

        assert_eq!(scale.then(shift).apply(1.0, 0.0), (12.0, 0.0));
    }

    /// Composition is not commutative, and getting the order backwards is
    /// the classic way to place content in the wrong spot.
    #[test]
    fn composition_order_matters() {
        let scale = Matrix::new(2.0, 0.0, 0.0, 2.0, 0.0, 0.0);
        let shift = Matrix::translate(10.0, 0.0);

        assert_eq!(shift.then(scale).apply(1.0, 0.0), (22.0, 0.0));
    }

    #[test]
    fn a_matrix_composed_with_its_inverse_is_the_identity() {
        let matrix = Matrix::new(2.0, 0.5, -1.0, 3.0, 7.0, -4.0);
        let round_trip = matrix.then(matrix.invert().expect("invertible"));

        assert!(close(round_trip.a, 1.0) && close(round_trip.d, 1.0));
        assert!(close(round_trip.b, 0.0) && close(round_trip.c, 0.0));
        assert!(close(round_trip.e, 0.0) && close(round_trip.f, 0.0));
    }

    #[test]
    fn a_collapsed_matrix_has_no_inverse() {
        assert!(Matrix::new(0.0, 0.0, 0.0, 0.0, 5.0, 5.0).invert().is_none());
    }

    #[test]
    fn the_bounding_box_of_an_unrotated_rect_is_that_rect() {
        let box_ = Matrix::IDENTITY.bounding_box(10.0, 20.0, 30.0, 40.0);

        assert_eq!(
            box_,
            Rect {
                x: 10.0,
                y: 20.0,
                width: 30.0,
                height: 40.0
            }
        );
    }

    /// A 90-degree rotation turns a wide box into a tall one. Mapping only
    /// two opposite corners would still produce a box here, just the wrong
    /// one — this is the case that catches that shortcut.
    #[test]
    fn the_bounding_box_of_a_rotated_rect_covers_every_corner() {
        let rotate_90 = Matrix::new(0.0, 1.0, -1.0, 0.0, 0.0, 0.0);

        let box_ = rotate_90.bounding_box(0.0, 0.0, 10.0, 2.0);

        assert!(close(box_.width, 2.0) && close(box_.height, 10.0));
        assert!(close(box_.x, -2.0) && close(box_.y, 0.0));
    }

    #[test]
    fn placing_the_unit_square_lands_it_on_the_target_rect() {
        let target = Rect {
            x: 5.0,
            y: 6.0,
            width: 100.0,
            height: 50.0,
        };

        let placed = Matrix::placing_unit_square(target).bounding_box(0.0, 0.0, 1.0, 1.0);

        assert_eq!(placed, target);
    }
}
