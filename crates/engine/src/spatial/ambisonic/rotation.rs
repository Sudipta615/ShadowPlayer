//! Bus frame rotation for ambisonics up to order 9 using Wigner-d matrices.

use super::basis::{sh_n, MAX_AMBISONIC_ORDER};
use crate::spatial::math::{Quat, Vec3};

/// Rotate an order-1 FOA bus frame by `q` (order-1 rotation: `W` invariant,
/// the `X Y Z` channels rotate exactly like direction vectors). `q` is the
/// world-space rotation to apply to the field.
pub fn rotate_bus_frame(q: Quat, frame: &mut [f32; 4]) {
    let v = Vec3::new(frame[3], frame[1], frame[2]);
    let r = q.rotate_vec3(v);
    frame[1] = r.y;
    frame[2] = r.z;
    frame[3] = r.x;
}

/// Highest bus order for which a rotation is implemented.
///
/// Orders 1–3 use the closed-form Wigner blocks (`wigner_block_2` /
/// `wigner_block_3`). Orders 4–9 recover the degree block from the same
/// ACN/N3D basis the encoder uses (see [`rotate_bus_frame_n`]); the recovery
/// is exact for the basis, not a truncated approximation.
pub const MAX_ROTATABLE_ORDER: u8 = MAX_AMBISONIC_ORDER;

/// Whether [`rotate_bus_frame_n`] performs an exact rotation for `order`.
pub fn rotation_supported(order: u8) -> bool {
    order <= MAX_ROTATABLE_ORDER
}

/// Rotate a bus frame of any supported order by `q`. Order 1 matches
/// [`rotate_bus_frame`]; order 2 additionally rotates the second-order
/// block (ACN 4–8) by its exact 5×5 Wigner matrix; order 3 rotates the
/// third-order block (ACN 9–15) by its exact 7×7 Wigner matrix; orders 4–9
/// rotate each degree block by a matrix recovered from the encoder's own
/// real-SH basis. `frame` must hold at least `channel_count(order)` values.
///
/// The order-4+ recovery is a least-squares solve over a deterministic
/// Fibonacci sphere, not a closed-form Wigner-D table. It uses the same
/// `sh_n` basis function as `encode_plane_wave_n`, so a rotated bus frame
/// is exactly the frame you get by encoding the rotated direction; the
/// result is allocation-free and runs in the audio path.
pub fn rotate_bus_frame_n(q: Quat, order: u8, frame: &mut [f32]) {
    let o = order.min(MAX_ROTATABLE_ORDER);
    assert!(
        frame.len() >= (o as usize + 1) * (o as usize + 1),
        "rotate_bus_frame_n: frame too short for order {o}"
    );

    // Degree 1: the directional channels rotate like vectors.
    if o >= 1 {
        let v = Vec3::new(frame[3], frame[1], frame[2]);
        let r = q.rotate_vec3(v);
        frame[1] = r.y;
        frame[2] = r.z;
        frame[3] = r.x;
    }
    // Degree 2: exact closed-form 5×5 Wigner block.
    if o >= 2 {
        let w2 = wigner_block_2(q);
        let c = [frame[4], frame[5], frame[6], frame[7], frame[8]];
        let mut out5 = [0.0f32; 5];
        for (i, &ci) in c.iter().enumerate() {
            for j in 0..5 {
                out5[j] += w2[i][j] * ci;
            }
        }
        frame[4] = out5[0];
        frame[5] = out5[1];
        frame[6] = out5[2];
        frame[7] = out5[3];
        frame[8] = out5[4];
    }
    // Degree 3: exact closed-form 7×7 Wigner block.
    if o >= 3 {
        let w3 = wigner_block_3(q);
        let c3 = [
            frame[9], frame[10], frame[11], frame[12], frame[13], frame[14], frame[15],
        ];
        let mut out7 = [0.0f32; 7];
        for (i, out) in out7.iter_mut().enumerate() {
            let mut acc = 0.0f32;
            for j in 0..7 {
                acc += w3[i][j] * c3[j];
            }
            *out = acc;
        }
        frame[9] = out7[0];
        frame[10] = out7[1];
        frame[11] = out7[2];
        frame[12] = out7[3];
        frame[13] = out7[4];
        frame[14] = out7[5];
        frame[15] = out7[6];
    }
    // Degrees 4–9: recover each degree block from the encoder's basis.
    for degree in 4..=o {
        rotate_degree_numeric(q, degree, frame);
    }
}

/// Largest single-degree block (`2·9+1`) and the largest oversampled set the
/// numeric rotation needs. Kept as constants so the hot path stays on the
/// stack: the realtime contract forbids heap allocation on the audio thread.
const MAX_DEGREE_CHANNELS: usize = 19;
const MAX_ROTATION_AUG: usize = 2 * MAX_DEGREE_CHANNELS;

/// Rotate one degree block (`degree >= 4`, ACN channels `d²..d²+2d`) in
/// place, recovering the (2d+1)×(2d+1) matrix by least squares.
///
/// For `Y_sj = Y_j(x_s)` and `Y'_sj = Y_j(R x_s)`, the row-convention matrix
/// solves `Y X = Y'`. Solving the normal equations `YᵀY X = YᵀY'` is enough
/// here because the basis is a degree-`d` polynomial basis and the sample
/// count exceeds the block width; the result is exact up to f64 round-off.
/// The frame is a column vector, so it is updated with `Xᵀ`, matching the
/// closed-form Wigner blocks' convention. `frame` is left untouched if the
/// system is singular.
fn rotate_degree_numeric(q: Quat, degree: u8, frame: &mut [f32]) {
    let d = degree as usize;
    let n = 2 * d + 1;
    let start = d * d;
    if n > MAX_DEGREE_CHANNELS || frame.len() < start + n {
        return;
    }

    let r = rotation_matrix_f64(q);
    let nsamp = n + 2;
    let mut gram = [[0.0f64; MAX_DEGREE_CHANNELS]; MAX_DEGREE_CHANNELS];
    let mut rhs = [[0.0f64; MAX_DEGREE_CHANNELS]; MAX_DEGREE_CHANNELS];

    for s in 0..nsamp {
        let (x, y, z) = fibonacci_point(s, nsamp);
        let basis = degree_basis(degree, x, y, z);

        // Rotate the sample points forward. Then `Y(R x) = Y(x) R`, so the
        // least-squares solution is the coefficient rotation directly.
        let rx = r[0][0] * x + r[0][1] * y + r[0][2] * z;
        let ry = r[1][0] * x + r[1][1] * y + r[1][2] * z;
        let rz = r[2][0] * x + r[2][1] * y + r[2][2] * z;
        let rotated = degree_basis(degree, rx, ry, rz);

        for j in 0..n {
            for k in 0..n {
                gram[j][k] += basis[j] * basis[k];
            }
            for i in 0..n {
                rhs[j][i] += basis[j] * rotated[i];
            }
        }
    }

    // Solve G · X = B for X (n right-hand sides) with Gauss–Jordan.
    let mut aug = [[0.0f64; MAX_ROTATION_AUG]; MAX_DEGREE_CHANNELS];
    for j in 0..n {
        for k in 0..n {
            aug[j][k] = gram[j][k];
        }
        for i in 0..n {
            aug[j][n + i] = rhs[j][i];
        }
    }
    if !gauss_jordan(&mut aug, n) {
        return;
    }

    // `aug[j][n + i]` is X[j][i] from `Y X = Y'`. A frame is a column, so the
    // update is f' = Xᵀ f, not f' = X f.
    let mut out = [0.0f32; MAX_DEGREE_CHANNELS];
    for k in 0..n {
        let mut acc = 0.0f64;
        for i in 0..n {
            acc += aug[i][n + k] * frame[start + i] as f64;
        }
        out[k] = acc as f32;
    }
    frame[start..start + n].copy_from_slice(&out[..n]);
}

/// A deterministic, approximately uniform point on the unit sphere.
fn fibonacci_point(index: usize, count: usize) -> (f64, f64, f64) {
    let z = 1.0 - 2.0 * (index as f64 + 0.5) / count as f64;
    let radius = (1.0 - z * z).max(0.0).sqrt();
    let phi = std::f64::consts::PI * (3.0 - 5.0f64.sqrt()) * index as f64;
    (radius * phi.cos(), radius * phi.sin(), z)
}

/// Evaluate only the `degree` block of the ACN/N3D basis, in f64.
fn degree_basis(degree: u8, x: f64, y: f64, z: f64) -> [f64; MAX_DEGREE_CHANNELS] {
    let mut buf = [0.0f32; 100];
    sh_n(degree, Vec3::new(x as f32, y as f32, z as f32), &mut buf);
    let start = (degree as usize) * (degree as usize);
    let count = 2 * degree as usize + 1;
    let mut out = [0.0f64; MAX_DEGREE_CHANNELS];
    for (i, slot) in out.iter_mut().take(count).enumerate() {
        *slot = buf[start + i] as f64;
    }
    out
}

/// In-place Gauss–Jordan elimination with partial pivoting on an
/// `n × 2n` augmented matrix. Returns `false` if the left block is singular.
#[allow(
    clippy::needless_range_loop,
    reason = "Each index addresses the pivot row and a target row of the same \
              augmented matrix; iterator forms obscure the elimination order."
)]
fn gauss_jordan(aug: &mut [[f64; MAX_ROTATION_AUG]; MAX_DEGREE_CHANNELS], n: usize) -> bool {
    for col in 0..n {
        let mut pivot = col;
        let mut best = aug[col][col].abs();
        for row in (col + 1)..n {
            let value = aug[row][col].abs();
            if value > best {
                best = value;
                pivot = row;
            }
        }
        if best < 1e-12 {
            return false;
        }
        if pivot != col {
            aug.swap(col, pivot);
        }
        let diagonal = aug[col][col];
        for k in col..(2 * n) {
            aug[col][k] /= diagonal;
        }
        for row in 0..n {
            if row == col {
                continue;
            }
            let factor = aug[row][col];
            if factor == 0.0 {
                continue;
            }
            for k in col..(2 * n) {
                aug[row][k] -= factor * aug[col][k];
            }
        }
    }
    true
}

/// The 3×3 rotation matrix for `q` (`R · v` rotates `v` by `q`, matching
/// [`Quat::rotate_vec3`]). f64 for the order-2 block's exactness.
pub fn rotation_matrix_f64(q: Quat) -> [[f64; 3]; 3] {
    let q = q.normalized().unwrap_or(Quat::IDENTITY);
    let (x, y, z, w) = (q.x as f64, q.y as f64, q.z as f64, q.w as f64);
    [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - w * z),
            2.0 * (x * z + w * y),
        ],
        [
            2.0 * (x * y + w * z),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - w * x),
        ],
        [
            2.0 * (x * z - w * y),
            2.0 * (y * z + w * x),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ]
}

/// The exact order-2 Wigner rotation block for the real-SH basis (ACN 4–8).
pub fn wigner_block_2(q: Quat) -> [[f32; 5]; 5] {
    let r = rotation_matrix_f64(q);
    let s15 = 15.0f64.sqrt();
    let s5h = 5.0f64.sqrt() * 0.5;
    let s15h = s15 * 0.5;
    let basis: [[[f64; 3]; 3]; 5] = [
        [
            [0.0, s15 * 0.5, 0.0],
            [s15 * 0.5, 0.0, 0.0],
            [0.0, 0.0, 0.0],
        ],
        [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, s15 * 0.5],
            [0.0, s15 * 0.5, 0.0],
        ],
        [[-s5h, 0.0, 0.0], [0.0, -s5h, 0.0], [0.0, 0.0, 2.0 * s5h]],
        [
            [0.0, 0.0, s15 * 0.5],
            [0.0, 0.0, 0.0],
            [s15 * 0.5, 0.0, 0.0],
        ],
        [[s15h, 0.0, 0.0], [0.0, -s15h, 0.0], [0.0, 0.0, 0.0]],
    ];
    let mut w = [[0.0f32; 5]; 5];
    for (i, fi) in basis.iter().enumerate() {
        let mut rf = [[0.0f64; 3]; 3];
        for a in 0..3 {
            for b in 0..3 {
                let mut s = 0.0;
                for c in 0..3 {
                    s += r[a][c] * fi[c][b];
                }
                rf[a][b] = s;
            }
        }
        let mut rfr = [[0.0f64; 3]; 3];
        for a in 0..3 {
            for b in 0..3 {
                let mut s = 0.0;
                for c in 0..3 {
                    s += rf[a][c] * r[b][c];
                }
                rfr[a][b] = s;
            }
        }
        for (j, fj) in basis.iter().enumerate() {
            let mut ip = 0.0;
            for a in 0..3 {
                for b in 0..3 {
                    ip += rfr[a][b] * fj[a][b];
                }
            }
            w[i][j] = (ip / 7.5) as f32;
        }
    }
    w
}

/// The exact order-3 Wigner rotation block for the real-SH basis (ACN 9–15).
pub fn wigner_block_3(q: Quat) -> [[f32; 7]; 7] {
    const M: usize = 10;
    // Cubic monomial exponents (x,y,z): x³ x²y x²z xy² xyz xz² y³ y²z yz² z³.
    const EXPS: [[usize; 3]; M] = [
        [3, 0, 0],
        [2, 1, 0],
        [2, 0, 1],
        [1, 2, 0],
        [1, 1, 1],
        [1, 0, 2],
        [0, 3, 0],
        [0, 2, 1],
        [0, 1, 2],
        [0, 0, 3],
    ];
    // N3D constants.
    let s358 = (35.0f64 / 8.0).sqrt(); // √(35/8)
    let s105 = 105.0f64.sqrt();
    let s218 = (21.0f64 / 8.0).sqrt();
    let s7h = 7.0f64.sqrt() * 0.5;
    let s105h = 105.0f64.sqrt() * 0.5;
    // The 7 basis cubics as 10-vectors. ACN 9..15.
    let basis: [[f64; M]; 7] = [
        // y(3x²−y²) = 3x²y −y³  (ACN 9, m = −3)
        [0.0, 3.0 * s358, 0.0, 0.0, 0.0, 0.0, -s358, 0.0, 0.0, 0.0],
        // xyz
        [0.0, 0.0, 0.0, 0.0, s105, 0.0, 0.0, 0.0, 0.0, 0.0],
        // y(5z²−1) → 4yz² −x²y −y³
        [0.0, -s218, 0.0, 0.0, 0.0, 0.0, -s218, 0.0, 4.0 * s218, 0.0],
        // z(5z²−3) → 2z³ −3x²z −3y²z
        [
            0.0,
            0.0,
            -3.0 * s7h,
            0.0,
            0.0,
            0.0,
            0.0,
            -3.0 * s7h,
            0.0,
            2.0 * s7h,
        ],
        // x(5z²−1) → 4xz² −x³ −xy²
        [-s218, 0.0, 0.0, -s218, 0.0, 4.0 * s218, 0.0, 0.0, 0.0, 0.0],
        // z(x²−y²) = x²z −y²z
        [0.0, 0.0, s105h, 0.0, 0.0, 0.0, 0.0, -s105h, 0.0, 0.0],
        // x(x²−3y²) = x³ −3xy²  (ACN 15, m = +3)
        [s358, 0.0, 0.0, -3.0 * s358, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    ];

    let r = rotation_matrix_f64(q);
    let mut sub = [[0.0f64; M]; M];
    for (mi, &[a, b, c]) in EXPS.iter().enumerate() {
        let mut stack = [(0usize, 0usize, 0usize, [0usize; 3], 0.0f64); 32];
        let mut stack_len = 0;
        stack[stack_len] = (a, b, c, [0, 0, 0], 1.0);
        stack_len += 1;

        while stack_len > 0 {
            stack_len -= 1;
            let (ra, rb, rc, expt, coeff) = stack[stack_len];
            if ra + rb + rc == 0 {
                let idx = EXPS.iter().position(|&e| e == expt).unwrap();
                sub[idx][mi] += coeff;
                continue;
            }
            if ra > 0 {
                for comp in 0..3 {
                    let mut e2 = expt;
                    e2[comp] += 1;
                    stack[stack_len] = (ra - 1, rb, rc, e2, coeff * r[0][comp]);
                    stack_len += 1;
                }
            } else if rb > 0 {
                for comp in 0..3 {
                    let mut e2 = expt;
                    e2[comp] += 1;
                    stack[stack_len] = (ra, rb - 1, rc, e2, coeff * r[1][comp]);
                    stack_len += 1;
                }
            } else {
                for comp in 0..3 {
                    let mut e2 = expt;
                    e2[comp] += 1;
                    stack[stack_len] = (ra, rb, rc - 1, e2, coeff * r[2][comp]);
                    stack_len += 1;
                }
            }
        }
    }

    let dot = |u: [f64; M], v: [f64; M]| (0..M).map(|k| u[k] * v[k]).sum::<f64>();
    let mut g = [[0.0f64; 7]; 7];
    let mut q_mat = [[0.0f64; 7]; 7];
    for i in 0..7 {
        let mut shi = [0.0f64; M];
        for n in 0..M {
            for mm in 0..M {
                shi[n] += sub[n][mm] * basis[i][mm];
            }
        }
        for j in 0..7 {
            g[i][j] = dot(basis[i], basis[j]);
            q_mat[i][j] = dot(shi, basis[j]);
        }
    }
    // Invert G.
    let mut gin = [[0.0f64; 7]; 7];
    gin.iter_mut().enumerate().for_each(|(i, row)| row[i] = 1.0);
    let mut gg = g;
    for c in 0..7 {
        let mut p = c;
        for r in c..7 {
            if gg[r][c].abs() > gg[p][c].abs() {
                p = r;
            }
        }
        gg.swap(c, p);
        gin.swap(c, p);
        let d = gg[c][c];
        for k in 0..7 {
            gg[c][k] /= d;
            gin[c][k] /= d;
        }
        for r in 0..7 {
            if r == c {
                continue;
            }
            let f = gg[r][c];
            for k in 0..7 {
                gg[r][k] -= f * gg[c][k];
                gin[r][k] -= f * gin[c][k];
            }
        }
    }
    // W₃ = Q·G⁻¹.
    let mut w = [[0.0f32; 7]; 7];
    for i in 0..7 {
        for j in 0..7 {
            let mut acc = 0.0;
            for p in 0..7 {
                acc += q_mat[i][p] * gin[p][j];
            }
            w[i][j] = acc as f32;
        }
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The order-2 numeric recovery must reproduce the closed-form Wigner
    /// block. This pins the row/column convention: `rotate_degree_numeric`
    /// solves `Y X = Y'` and applies `Xᵀ` to the frame, exactly as
    /// `rotate_bus_frame_n` applies `Wᵀ` to the Wigner block.
    #[test]
    fn numeric_recovery_matches_the_closed_form_degree_two_block() {
        let q = Quat::from_euler_rad(0.7, 0.3, -0.2);
        let mut numeric = [0.0f32; 9];
        let mut closed = [0.0f32; 9];
        for (i, (a, b)) in numeric.iter_mut().zip(closed.iter_mut()).enumerate() {
            let v = (i as f32 * 0.37 - 0.8).sin();
            *a = v;
            *b = v;
        }

        rotate_degree_numeric(q, 2, &mut numeric);

        let w2 = wigner_block_2(q);
        let c = [closed[4], closed[5], closed[6], closed[7], closed[8]];
        let mut expected = [0.0f32; 5];
        for (i, &ci) in c.iter().enumerate() {
            for j in 0..5 {
                expected[j] += w2[i][j] * ci;
            }
        }
        for k in 0..5 {
            assert!(
                (numeric[4 + k] - expected[k]).abs() < 2e-4,
                "ACN {}: numeric {} vs closed {}",
                4 + k,
                numeric[4 + k],
                expected[k]
            );
        }
    }
}
