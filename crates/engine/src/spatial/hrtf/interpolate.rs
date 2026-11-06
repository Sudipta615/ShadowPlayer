//! Spherical triangulation and barycentric interpolation on the unit sphere (spec §47–48, §62).

use crate::spatial::math::Vec3;

/// A spherical triangle defined by three vertex indices on the unit sphere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SphericalTriangle {
    pub a: usize,
    pub b: usize,
    pub c: usize,
}

impl SphericalTriangle {
    pub const fn new(a: usize, b: usize, c: usize) -> Self {
        Self { a, b, c }
    }
}

/// Compute spherical barycentric coordinates `[w0, w1, w2]` for `query` with
/// respect to the spherical triangle `(v0, v1, v2)`.
///
/// Returns `Some([w0, w1, w2])` if `query` projects within or on the spherical triangle
/// (with sum of weights normalized to 1.0), or `None` if `query` is outside.
pub fn barycentric_sphere(query: Vec3, v0: Vec3, v1: Vec3, v2: Vec3) -> Option<[f32; 3]> {
    // Solve [v0 v1 v2] * [c0, c1, c2]^T = query
    // Using Cramer's rule: det = v0 . (v1 x v2)
    let det = v0.dot(v1.cross(v2));
    if det.abs() < 1e-6 {
        return None;
    }

    let c0 = query.dot(v1.cross(v2)) / det;
    let c1 = v0.dot(query.cross(v2)) / det;
    let c2 = v0.dot(v1.cross(query)) / det;

    const EPS: f32 = -1e-4;
    if c0 >= EPS && c1 >= EPS && c2 >= EPS {
        let sum = (c0 + c1 + c2).max(1e-12);
        let w0 = (c0 / sum).max(0.0);
        let w1 = (c1 / sum).max(0.0);
        let w2 = (c2 / sum).max(0.0);
        let norm = (w0 + w1 + w2).max(1e-12);
        Some([w0 / norm, w1 / norm, w2 / norm])
    } else {
        None
    }
}

/// Compute 3D convex hull / spherical Delaunay triangulation for points on the unit sphere.
/// For points on S^2, the convex hull facets form the exact spherical Delaunay triangulation.
///
/// Implemented using an incremental 3D convex hull algorithm running in O(N^2) worst-case
/// time rather than O(N^4), allowing standard SOFA datasets (N >= 1,000) to triangulate in
/// milliseconds instead of stalling the process.
pub fn triangulate_sphere(points: &[Vec3]) -> Vec<SphericalTriangle> {
    let n = points.len();
    if n < 4 {
        return Vec::new();
    }

    // Step 1: Find 4 non-coplanar points to form the initial tetrahedron
    let i0 = 0;
    let mut i1 = 0;
    let mut max_d = -1.0f32;
    for i in 1..n {
        let d = (points[i] - points[i0]).length();
        if d > max_d {
            max_d = d;
            i1 = i;
        }
    }
    if max_d < 1e-4 {
        return Vec::new();
    }

    let mut i2 = 0;
    let mut max_area = -1.0f32;
    let v01 = points[i1] - points[i0];
    for i in 0..n {
        if i == i0 || i == i1 {
            continue;
        }
        let cr = v01.cross(points[i] - points[i0]);
        let area = cr.length();
        if area > max_area {
            max_area = area;
            i2 = i;
        }
    }
    if max_area < 1e-4 {
        return Vec::new();
    }

    let mut i3 = 0;
    let mut max_vol = -1.0f32;
    let n012 = v01.cross(points[i2] - points[i0]);
    for i in 0..n {
        if i == i0 || i == i1 || i == i2 {
            continue;
        }
        let vol = n012.dot(points[i] - points[i0]).abs();
        if vol > max_vol {
            max_vol = vol;
            i3 = i;
        }
    }
    if max_vol < 1e-4 {
        return Vec::new();
    }

    // Centroid of initial tetrahedron
    let centroid = (points[i0] + points[i1] + points[i2] + points[i3]) * 0.25;

    struct Face {
        a: usize,
        b: usize,
        c: usize,
        normal: Vec3,
    }

    let make_face = |a: usize, b: usize, c: usize| -> Face {
        let mut nm = (points[b] - points[a]).cross(points[c] - points[a]);
        // Ensure normal points outward from the interior centroid
        if nm.dot(points[a] - centroid) < 0.0 {
            nm = -nm;
            Face {
                a,
                b: c,
                c: b,
                normal: nm.normalized().unwrap_or(Vec3::Y),
            }
        } else {
            Face {
                a,
                b,
                c,
                normal: nm.normalized().unwrap_or(Vec3::Y),
            }
        }
    };

    let mut faces = vec![
        make_face(i0, i1, i2),
        make_face(i0, i2, i3),
        make_face(i0, i3, i1),
        make_face(i1, i3, i2),
    ];

    let tet_indices = [i0, i1, i2, i3];

    // Incremental hull: process each remaining point
    for i in 0..n {
        if tet_indices.contains(&i) {
            continue;
        }
        let pt = points[i];

        // Find visible faces
        let mut any_visible = false;
        for f in &faces {
            if f.normal.dot(pt - points[f.a]) > 1e-6 {
                any_visible = true;
                break;
            }
        }
        if !any_visible {
            continue;
        }

        let mut visible = Vec::new();
        let mut non_visible = Vec::new();
        for f in faces.drain(..) {
            if f.normal.dot(pt - points[f.a]) > 1e-6 {
                visible.push(f);
            } else {
                non_visible.push(f);
            }
        }

        // Find horizon edges: edges belonging to visible faces that are not shared by two visible faces
        use std::collections::HashMap;
        let mut edge_count: HashMap<(usize, usize), (usize, usize, u8)> = HashMap::new();
        for f in &visible {
            let edges = [(f.a, f.b), (f.b, f.c), (f.c, f.a)];
            for (u, v) in edges {
                let key = if u < v { (u, v) } else { (v, u) };
                let entry = edge_count.entry(key).or_insert((u, v, 0));
                entry.2 += 1;
            }
        }

        faces = non_visible;
        for (_, (u, v, count)) in edge_count {
            if count == 1 {
                faces.push(make_face(u, v, i));
            }
        }
    }

    faces
        .into_iter()
        .map(|f| SphericalTriangle::new(f.a, f.b, f.c))
        .collect()
}

/// Spherical HRTF interpolator for irregular direction sets.
#[derive(Debug, Clone)]
pub struct SphericalHrtfInterpolator {
    points: Vec<Vec3>,
    triangles: Vec<SphericalTriangle>,
}

impl SphericalHrtfInterpolator {
    /// Build an interpolator from a set of unit measurement directions.
    pub fn new(directions: &[Vec3]) -> Self {
        let points = directions
            .iter()
            .map(|&d| d.normalized().unwrap_or(Vec3::Y))
            .collect::<Vec<_>>();
        let triangles = triangulate_sphere(&points);
        Self { points, triangles }
    }

    /// Number of measurement directions.
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// True if there are no measurement directions.
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// The measurement directions, as normalized unit vectors.
    pub fn points(&self) -> &[Vec3] {
        &self.points
    }

    /// Find enclosing triangle and barycentric weights for a query direction.
    /// Falls back to 3 nearest neighbors without heap allocations if no spherical triangle contains the query directly.
    pub fn find_weights(&self, query: Vec3) -> (SphericalTriangle, [f32; 3]) {
        let q = query.normalized().unwrap_or(Vec3::Y);

        // First attempt: check spherical triangles
        for &tri in &self.triangles {
            let v0 = self.points[tri.a];
            let v1 = self.points[tri.b];
            let v2 = self.points[tri.c];
            if let Some(w) = barycentric_sphere(q, v0, v1, v2) {
                return (tri, w);
            }
        }

        // Realtime-safe zero-allocation fallback: 3 nearest neighbors by cosine distance (dot product)
        let mut best_i = [0usize; 3];
        let mut best_d = [f32::INFINITY; 3];

        for (idx, &p) in self.points.iter().enumerate() {
            let d = 1.0 - q.dot(p);
            if d < best_d[0] {
                best_d[2] = best_d[1];
                best_i[2] = best_i[1];
                best_d[1] = best_d[0];
                best_i[1] = best_i[0];
                best_d[0] = d;
                best_i[0] = idx;
            } else if d < best_d[1] {
                best_d[2] = best_d[1];
                best_i[2] = best_i[1];
                best_d[1] = d;
                best_i[1] = idx;
            } else if d < best_d[2] {
                best_d[2] = d;
                best_i[2] = idx;
            }
        }

        let d0 = best_d[0].max(1e-6);
        let d1 = best_d[1].max(1e-6);
        let d2 = best_d[2].max(1e-6);

        let inv0 = 1.0 / d0;
        let inv1 = 1.0 / d1;
        let inv2 = 1.0 / d2;
        let sum = inv0 + inv1 + inv2;

        (
            SphericalTriangle::new(best_i[0], best_i[1], best_i[2]),
            [inv0 / sum, inv1 / sum, inv2 / sum],
        )
    }

    /// Interpolate HRTF impulse responses for `query` into `out`.
    pub fn interpolate(
        &self,
        query: Vec3,
        irs: &[f32],
        taps: usize,
        stride: usize,
        ear_offset: usize,
        out: &mut [f32],
    ) {
        debug_assert!(out.len() >= taps);
        let (tri, w) = self.find_weights(query);

        let base_a = tri.a * stride + ear_offset;
        let base_b = tri.b * stride + ear_offset;
        let base_c = tri.c * stride + ear_offset;

        let ir_a = &irs[base_a..base_a + taps];
        let ir_b = &irs[base_b..base_b + taps];
        let ir_c = &irs[base_c..base_c + taps];

        for k in 0..taps {
            out[k] = w[0] * ir_a[k] + w[1] * ir_b[k] + w[2] * ir_c[k];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn barycentric_sphere_exact_at_vertices() {
        let v0 = Vec3::X;
        let v1 = Vec3::Y;
        let v2 = Vec3::Z;

        let w0 = barycentric_sphere(v0, v0, v1, v2).expect("v0 inside");
        assert!((w0[0] - 1.0).abs() < 1e-4);
        assert!(w0[1].abs() < 1e-4);
        assert!(w0[2].abs() < 1e-4);

        let w1 = barycentric_sphere(v1, v0, v1, v2).expect("v1 inside");
        assert!(w1[0].abs() < 1e-4);
        assert!((w1[1] - 1.0).abs() < 1e-4);
        assert!(w1[2].abs() < 1e-4);
    }

    #[test]
    fn barycentric_sphere_centroid() {
        let v0 = Vec3::X;
        let v1 = Vec3::Y;
        let v2 = Vec3::Z;
        let centroid = (v0 + v1 + v2).normalized().unwrap();

        let w = barycentric_sphere(centroid, v0, v1, v2).expect("centroid inside");
        assert!((w[0] - 1.0 / 3.0).abs() < 1e-4);
        assert!((w[1] - 1.0 / 3.0).abs() < 1e-4);
        assert!((w[2] - 1.0 / 3.0).abs() < 1e-4);
    }

    #[test]
    fn octahedron_triangulation() {
        let pts = vec![Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z];
        let interp = SphericalHrtfInterpolator::new(&pts);
        assert_eq!(interp.len(), 6);
        assert_eq!(interp.triangles.len(), 8); // 8 octants

        let query = Vec3::new(1.0, 1.0, 1.0).normalized().unwrap();
        let (_tri, weights) = interp.find_weights(query);
        assert!((weights.iter().sum::<f32>() - 1.0).abs() < 1e-4);
    }
}
