# H2b2 known-answer preparation — 15 October 2026

Preparation evidence only. This file is not `bench/dcp_validation/known-answers.json`, and it does not authorize H2b2. Implementing candidate conversion still needs a separate assignment after the H2b1 statistics import. H2b1's API was not used. No colour-pipeline source, expected-value JSON, ICC parser, white gate, build, device, cache, dependency, or Git mutation is included.

Author/model: Grok Build / `grok-4.7` (slot `01a13e02-3c22-7ad0-9ed3-8bab4bc7d945`). Snapshot: `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/harness-review`. Frozen HEAD `4c1e965a53de9c835df920b2e3d69c2c8a864708`, tree `9f36c891f35d9310a7977bda7d851178ad92039d`. Environment: macOS 26.5.2 / 25F84, arm64. Python 3.9.6, standard-library `decimal` only.

Contract read: `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/lead/docs/integration/H2-COMPARISON-CONTRACT-2026-10-15.md`, SHA-256 `f29936252b3ec103a41a76961d97902c82970416ea5802f88d497ef4341a6880`, sections 3, 6, and 7. Prior review read, not modified: `docs/integration/H2A-INDEPENDENT-REVIEW-2026-10-15.md`, SHA-256 `c9be23b548446ace92365dafa17f08f1362f8237d9451c355aecd53525aede0b`. This note follows the lead contract. It does not reopen the review's ICC, threshold, or package findings. Section 6 item 2 (synthetic ICC, gamma 2, `L≈57.07542089`, PCS-D50→Wc) is not this preparation.

## 1. Path these numbers describe

Candidate samples are already linear sRGB. Do not apply the sRGB electro-optical transfer function. Do not call `prophoto_to_lab`, `xyz_to_lab`, `bradford_adapt_matrix`, or any other production or harness converter, and do not invert `render.rs`'s row-normalized matrix. The numbers below were produced only by the script in §4.

1. Chromaticities to XYZ(D65), Y=1, then the 3×3 matrix `M` that maps a column of linear RGB to XYZ.
2. Linear Bradford `A` from D65 to the existing DNG white `Wc = [0.9642956764, 1, 0.8251046025]`.
3. Piecewise CIE Lab relative to that same `Wc`, matching the existing helper's definition: `t > (6/29)^3` uses the cube root; otherwise `t/(3*(6/29)^2) + 4/29`. Then `L = 116*fy-16`, `a = 500*(fx-fy)`, `b = 200*(fy-fz)`.

`Wc` is the 10-digit rounding of xy `(0.3457, 0.3585)` already stored as `D50_XYZ`. It is not W3C's full-quotient D50 and not ICC nominal `[0.9642, 1, 0.8249]`. Those whites stay distinct. The W3C sample's precomputed D65→D50 matrix uses the full quotient, so it is a cone-algebra check only and is not the fixture matrix.

Text of `delta_e.rs` lines 119–155 was read to confirm that definition. The function was not executed.

## 2. Sources checked

[W3C CSS Color 4, CRD 9 October 2026](https://www.w3.org/TR/2026/CRD-css-color-4-20261009/#color-conversion-code), fetched this session:

- D65 xy `(0.3127, 0.3290)` and D50 xy `(0.3457, 0.3585)`, four-figure table.
- sRGB primaries `(0.640, 0.330)`, `(0.300, 0.600)`, `(0.150, 0.060)`, white D65. The same primaries also appear on the display-p3 table; Rec.2020's green is different and was not used.
- Sample `lin_sRGB_to_XYZ` rationals, multiplying a column vector. Sample code is not normative. The sRGB transfer in that sample is not part of this candidate path.
- Sample text says linear Bradford, citing ICC, and prints one composed D65→D50 matrix. The page does not print the cone coefficients. Those coefficients are the contract's `B`, checked by reproducing that printed matrix (§5).

Contract §3 supplies `B = [[0.8951, 0.2664, -0.1614], [-0.7502, 1.7135, 0.0367], [0.0389, -0.0685, 1.0296]]` and `A = inv(B) * diag((B*Wdst)/(B*Wsrc)) * B`. No gamut mapping, black-point compensation, or integer PCS quantization.

## 3. Formula, rounding, and acceptance

`xy` to XYZ at Y=1 is `(x/y, 1, (1-x-y)/y)`. Unscaled primary columns form `P`. Solve `P * scales = W65` and set each column of `M` to that primary times its scale. `W65` is the D65 chromaticity quotient, not a rounded constant. `Wdst` is the exact 10-digit `Wc` strings.

Cube roots on the nonlinear branch use Newton iteration, cross-checked by `Decimal` power. The linear branch is cross-checked by the W3C form `ε = 216/24389`, `κ = 24389/27`, `(κ*t+16)/116`. Negatives stay on the linear branch. There is no clamp.

`Decimal` precision 80, rounding `ROUND_HALF_EVEN`. Published components are quantized to `1e-12` (12 digits after the decimal). That quantum is `5e-13`, far below the contract's analytic tolerance of absolute error `< 1e-6` per component. A correct f64 implementation should land well inside that tolerance; the test must parse these decimals and must not regenerate them by calling the function under test. Bitwise f64 equality is not required. A future implementation should recompute `M` and `A` in f64 from this formula rather than paste a rounded adaptation matrix.

White's analytic `a*` residual is `-1.5e-77`. Quantizing that residual prints `-0E-12`. The proposed fixture value is `0`.

## 4. Reproducible script

The first run was an inline `python3` heredoc. Its stdout was not saved. The confirmation run in this turn was the same script, with stdout redirected to `/tmp/h2b2-oracle-20261015/derive.out` (4795 bytes, SHA-256 `7bfa41f4cd4f6a09d117aa15abe9adbc389d675373dceace1e07f3ced3ded94f`). The script was not written into the repository. Scratch directory `/tmp/h2b2-oracle-20261015` holds that stdout only.

```python
from decimal import Decimal, getcontext, ROUND_HALF_EVEN
getcontext().prec = 80
getcontext().rounding = ROUND_HALF_EVEN

def D(x):
    return x if isinstance(x, Decimal) else Decimal(str(x))

def matvec(M, v):
    return tuple(sum(M[i][j] * v[j] for j in range(3)) for i in range(3))

def matmul(A, B):
    return tuple(tuple(sum(A[i][k] * B[k][j] for k in range(3)) for j in range(3)) for i in range(3))

def transpose(M):
    return tuple(tuple(M[j][i] for j in range(3)) for i in range(3))

def gauss_inverse(A):
    M = [[A[i][j] for j in range(3)] + [D(1) if i == j else D(0) for j in range(3)] for i in range(3)]
    for col in range(3):
        pivot = max(range(col, 3), key=lambda r: abs(M[r][col]))
        if M[pivot][col] == 0:
            raise ZeroDivisionError('singular')
        M[col], M[pivot] = M[pivot], M[col]
        div = M[col][col]
        M[col] = [v / div for v in M[col]]
        for r in range(3):
            if r == col:
                continue
            factor = M[r][col]
            M[r] = [M[r][c] - factor * M[col][c] for c in range(6)]
    return tuple(tuple(M[i][j] for j in range(3, 6)) for i in range(3))

def adjugate_inverse(A):
    a,b,c = A[0]
    d,e,f = A[1]
    g,h,i = A[2]
    det = a*(e*i - f*h) - b*(d*i - f*g) + c*(d*h - e*g)
    if det == 0:
        raise ZeroDivisionError('singular')
    C = (
        (e*i - f*h, -(d*i - f*g), d*h - e*g),
        (-(b*i - c*h), a*i - c*g, -(a*h - b*g)),
        (b*f - c*e, -(a*f - c*d), a*e - b*d),
    )
    adj = transpose(C)  # adjugate is transpose of cofactors
    return tuple(tuple(adj[r][c] / det for c in range(3)) for r in range(3))

def xy_xyz(x, y):
    x, y = D(x), D(y)
    return (x / y, D(1), (1 - x - y) / y)

prim = {'R': ('0.64', '0.33'), 'G': ('0.30', '0.60'), 'B': ('0.15', '0.06')}
W65 = xy_xyz('0.3127', '0.3290')
cols = [xy_xyz(*prim[c]) for c in 'RGB']
P = tuple(tuple(cols[j][i] for j in range(3)) for i in range(3))
scales = matvec(gauss_inverse(P), W65)
M = tuple(tuple(cols[j][i] * scales[j] for j in range(3)) for i in range(3))

W3C = (
    (D(506752)/D(1228815), D(87881)/D(245763), D(12673)/D(70218)),
    (D(87098)/D(409605), D(175762)/D(245763), D(12673)/D(175545)),
    (D(7918)/D(409605), D(87881)/D(737289), D(1001167)/D(1053270)),
)
max_m = max(abs(M[i][j] - W3C[i][j]) for i in range(3) for j in range(3))
print('RGB_to_XYZ max abs diff vs W3C rationals', max_m)

# second inverse method for scales only
scales_b = matvec(adjugate_inverse(P), W65)
print('scale method diff', max(abs(scales[i]-scales_b[i]) for i in range(3)))

B = (
    (D('0.8951'), D('0.2664'), D('-0.1614')),
    (D('-0.7502'), D('1.7135'), D('0.0367')),
    (D('0.0389'), D('-0.0685'), D('1.0296')),
)
Binv_g = gauss_inverse(B)
Binv_a = adjugate_inverse(B)
print('Bradford inverse method diff', max(abs(Binv_g[i][j]-Binv_a[i][j]) for i in range(3) for j in range(3)))

def adaptation(Wsrc, Wdst, Binv):
    src_c = matvec(B, Wsrc)
    dst_c = matvec(B, Wdst)
    scale = tuple(dst_c[i] / src_c[i] for i in range(3))
    SB = tuple(tuple(scale[i] * B[i][j] for j in range(3)) for i in range(3))
    return matmul(Binv, SB)

Wc = (D('0.9642956764'), D('1'), D('0.8251046025'))
A = adaptation(W65, Wc, Binv_g)
A2 = adaptation(W65, Wc, Binv_a)
print('adaptation matrix method diff', max(abs(A[i][j]-A2[i][j]) for i in range(3) for j in range(3)))
print('white maps to Wc residual', max(abs(x) for x in [matvec(A, W65)[i]-Wc[i] for i in range(3)]))

# Reproduce W3C printed D65->full D50 matrix as an independent algebra check. Not used for fixtures.
D50_full = xy_xyz('0.3457', '0.3585')
A_w3c_full = adaptation(W65, D50_full, Binv_g)
W3C_AD = (
    (D('1.0479297925449969'), D('0.022946870601609652'), D('-0.05019226628920524')),
    (D('0.02962780877005599'), D('0.9904344267538799'), D('-0.017073799063418826')),
    (D('-0.009243040646204504'), D('0.015055191490298152'), D('0.7518742814281371')),
)
print('vs W3C printed D65toD50 max diff', max(abs(A_w3c_full[i][j]-W3C_AD[i][j]) for i in range(3) for j in range(3)))
print('D50 full minus Wc', [D50_full[i]-Wc[i] for i in range(3)])

EPS = (D(6)/D(29))**3
THREE_D2 = D(3) * (D(6)/D(29))**2

def cbrt_pow(t):
    return t ** (D(1)/D(3))

def cbrt_newton(t):
    x = t if t > 1 else D(1)
    for _ in range(80):
        x = (D(2)*x + t/(x*x)) / D(3)
    return x

def f_piece(t):
    if t > EPS:
        return cbrt_newton(t)
    return t / THREE_D2 + D(4)/D(29)

def f_w3c(t):
    eps = D(216)/D(24389)
    kappa = D(24389)/D(27)
    if t > eps:
        return cbrt_pow(t)
    return (kappa * t + D(16)) / D(116)

def lab_of(xyz, f):
    fx, fy, fz = (f(xyz[i]/Wc[i]) for i in range(3))
    return (D(116)*fy - D(16), D(500)*(fx-fy), D(200)*(fy-fz))

probe = D('-0.02')
print('piecewise forms on negative', [f_piece(probe)-f_w3c(probe)])
print('cbrt methods', cbrt_newton(D('0.18'))-cbrt_pow(D('0.18')))
print('neutral L identity', D(116)*cbrt_newton(D('0.18'))-D(16))

def convert(rgb, matrix=M, adapt=A):
    xyz = matvec(matrix, rgb)
    xyz2 = matvec(adapt, xyz)
    return xyz, xyz2, lab_of(xyz2, f_piece)

samples = {
    'black': (D(0), D(0), D(0)),
    'white': (D(1), D(1), D(1)),
    'gray18': (D('0.18'), D('0.18'), D('0.18')),
    'primary_r': (D(1), D(0), D(0)),
    'primary_g': (D(0), D(1), D(0)),
    'primary_b': (D(0), D(0), D(1)),
    'chroma': (D('0.2'), D('0.4'), D('0.6')),
    'extended': (D('-0.1'), D('0.2'), D('1.2')),
}

def q(x):
    return x.quantize(D('1e-12'), rounding=ROUND_HALF_EVEN)

def show(name, rgb):
    xyz, xyz2, lab = convert(rgb)
    print(name)
    print('  rgb', rgb)
    print('  xyz_d65', [q(v) for v in xyz])
    print('  xyz_wc', [q(v) for v in xyz2])
    print('  lab', [q(v) for v in lab])

for n, rgb in samples.items():
    show(n, rgb[0] if False else rgb)

# wrong paths
A_rev = adaptation(Wc, W65, Binv_g)
I = ((D(1),D(0),D(0)),(D(0),D(1),D(0)),(D(0),D(0),D(1)))
Mt = transpose(M)
At = transpose(A)

def lab_only(xyz):
    return lab_of(xyz, f_piece)

def delta(a, b):
    return max(abs(a[i]-b[i]) for i in range(3))

print('--- discrimination ---')
for name in ('white', 'chroma', 'extended', 'primary_r', 'gray18'):
    rgb = samples[name]
    xyz, xyz2, lab = convert(rgb)
    omitted = lab_only(xyz)  # D65 XYZ into Lab
    reversed_lab = lab_only(matvec(A_rev, xyz))
    doubled = lab_only(matvec(A, xyz2))
    transposed_m = convert(rgb, matrix=Mt, adapt=A)[2]
    transposed_a = convert(rgb, matrix=M, adapt=At)[2]
    swapped = convert((rgb[2], rgb[1], rgb[0]))[2]
    clamped_rgb = tuple(min(D(1), max(D(0), c)) for c in rgb)
    clamped = convert(clamped_rgb)[2]
    print(name,
          'omit', q(delta(lab, omitted)),
          'rev', q(delta(lab, reversed_lab)),
          'dbl', q(delta(lab, doubled)),
          'Mt', q(delta(lab, transposed_m)),
          'At', q(delta(lab, transposed_a)),
          'swap', q(delta(lab, swapped)),
          'clamp', q(delta(lab, clamped)))

# identity checks
xyz, xyz2, lab = convert(samples['gray18'])
L_id = D(116)*cbrt_newton(D('0.18'))-D(16)
print('gray lab vs identity', [lab[i] - (L_id if i==0 else D(0)) for i in range(3)])
xyz, xyz2, lab = convert(samples['white'])
print('white lab', lab)
xyz, xyz2, lab = convert(samples['black'])
print('black lab', lab)
print('extended xyz_d65 signs', convert(samples['extended'])[0])
print('Wc used', Wc)
print('W65', W65)
```

## 5. Cross-checks

These are separate equations or a primary printed reference, not a second execution of the same solver.

| Check | Result |
|---|---|
| Chromaticity solve versus W3C rational `lin_sRGB_to_XYZ` | max abs `2e-80` |
| Gauss–Jordan scales versus adjugate scales | max abs `2e-80` |
| Bradford inverse, Gauss–Jordan versus adjugate | max abs `7e-80` |
| Adaptation matrix from those two inverses | max abs `5e-80` |
| `[1,1,1]` through `M` then `A` versus `Wc` | max abs `7e-80` |
| Same cone matrix and full-quotient D50 versus W3C's printed D65→D50 matrix | max abs `3.16e-16` (the printed matrix's decimal limit) |
| Full-quotient D50 minus `Wc` | X `+2.96e-11`, Y `0`, Z `+1.05e-11` |
| Piecewise `(6/29)` form versus W3C `κ/ε` form at `t = -0.02` | abs `1e-80` |
| Newton cube root versus `**(1/3)` at `0.18` | `0` at precision 80 |
| Gray Lab versus `L = 116*cbrt(0.18)-16`, `a = b = 0` | residual `0` at about `1e-78` |

The closed-form gray `L` is `49.49610761011958...`, which quantizes to `49.496107610120`. That matches the contract's `L≈49.49610761` and does not use `M` or `A`. The contract figure was not an input to the script.

## 6. Proposed future fixture values

Twelve digits after the decimal, `ROUND_HALF_EVEN`. `xyz_d65` is before adaptation. `xyz_wc` is after Bradford toward `Wc`. Future absolute tolerance: `< 1e-6` on each component. This block is a proposal for a later assignment. It is not a file to add now.

```json
{
  "tolerance_abs": "1e-6",
  "samples": [
    {"name": "black", "rgb": [0, 0, 0], "xyz_d65": [0, 0, 0], "xyz_wc": [0, 0, 0], "lab": [0, 0, 0]},
    {"name": "white", "rgb": [1, 1, 1], "xyz_d65": [0.950455927052, 1.000000000000, 1.089057750760], "xyz_wc": [0.964295676400, 1.000000000000, 0.825104602500], "lab": [100, 0, 0]},
    {"name": "gray18", "rgb": [0.18, 0.18, 0.18], "xyz_d65": [0.171082066869, 0.180000000000, 0.196030395137], "xyz_wc": [0.173573221752, 0.180000000000, 0.148518828450], "lab": [49.496107610120, 0, 0]},
    {"name": "primary_r", "rgb": [1, 0, 0], "xyz_d65": [0.412390799266, 0.212639005872, 0.019330818716], "xyz_wc": [0.436065746863, 0.222493177106, 0.013923921463], "lab": [54.290541404228, 80.804928172989, 69.890964767746]},
    {"name": "primary_g", "rgb": [0, 1, 0], "xyz_d65": [0.357584339384, 0.715168678768, 0.119194779795], "xyz_wc": [0.385151509574, 0.716887013099, 0.097081324231], "lab": [87.818534365260, -79.271060777848, 80.994581484389]},
    {"name": "primary_b", "rgb": [0, 0, 1], "xyz_d65": [0.180480788402, 0.072192315361, 0.950532152250], "xyz_wc": [0.143078419963, 0.060619809794, 0.714099356806], "lab": [29.568301978949, 68.287364604347, -112.029709525243]},
    {"name": "chroma", "rgb": [0.2, 0.4, 0.6], "xyz_d65": [0.333800368648, 0.371910661898, 0.621863367011], "xyz_wc": [0.327120805180, 0.367625326538, 0.470076928068], "lab": [67.098489639706, -9.469196039036, -22.526275571832]},
    {"name": "extended", "rgb": [-0.1, 0.2, 1.2], "xyz_d65": [0.246854734032, 0.208400613599, 1.162544456787], "xyz_wc": [0.205117831184, 0.193871856662, 0.874943100867], "lab": [51.137151709233, 9.085989427183, -88.194671466775]}
  ]
}
```

## 7. What the selected fixtures catch

Each cell is the maximum absolute Lab-component difference from the correct path, quantized to 12 digits. Zero means that sample does not witness that fault. Black is all zeros on every fault and is not a witness. Primary green and primary blue were not run through this fault table; primary red is the unequal-channel primary witness.

Fault definitions used here:

- omit: D65 XYZ goes straight into Lab relative to `Wc`
- reversed: Bradford is built `Wc → D65`, then Lab still uses `Wc`
- doubled: `A` is applied twice, then Lab uses `Wc`
- `M` transpose: `M` is transposed and still multiplies a column vector
- `A` transpose: the same for `A`
- channel swap: input `(R,G,B)` is replaced by `(B,G,R)` before the correct matrices
- clamp: each RGB channel is forced into `[0,1]` before the correct matrices

| Sample | omit | reversed | doubled | M transpose | A transpose | R↔B swap | RGB clamp |
|---|---:|---:|---:|---:|---:|---:|---:|
| white | 19.386868530838 | 40.401861875214 | 17.865720390552 | 91.584738868839 | 7.805195879611 | 0 | 0 |
| gray18 | 10.946245064819 | 22.811764595071 | 10.087371941654 | 51.710723382743 | 4.406982321113 | 0 | 0 |
| primary_r | 7.744869992808 | 15.778391337356 | 7.572734281622 | 55.198832595156 | 41.813101659300 | 181.920674292989 | 0 |
| chroma | 15.654544311545 | 32.600501330961 | 14.438254010936 | 86.849336032102 | 5.338016102498 | 51.171920675122 | 0 |
| extended | 17.443864004468 | 36.164455106004 | 16.225445320329 | 349.758033412904 | 8.023236527475 | 275.098102645411 | 13.621693774145 |

Every adaptation and transpose fault above is much larger than `1e-6` on white, gray, chroma, and primary red. Channel swap needs an unequal sample (chroma or primary red); white and gray do not show it. The RGB `[0,1]` clamp is shown only by `[-0.1, 0.2, 1.2]`.

Limits of this set, so a later test does not over-claim: extended D65 XYZ is entirely positive (about `0.247, 0.208, 1.163`), so a clamp of negative XYZ would not move it. Every proposed `L*` is inside `[0,100]` (black is 0 and white is 100), so a clamp of `L*` into that interval is also invisible here.

## 8. Receipt

**Pass — static derivation.** Frozen HEAD and tree matched. Contract hash matched before the derivation. Both script runs printed the same fixture and fault figures. W3C chromaticities, rational RGB→XYZ matrix, and printed full-D50 adaptation matrix were read from the CRD. No production or harness conversion function was called.

**Fail:** none in this preparation.

**Not exercised:** Rust tests and builds, candidate image routes, Adobe exports, ICC parsing, the PCS-D50→Wc reference path, H2b1 statistics, devices, and performance.

Commands: `git rev-parse HEAD` and `git rev-parse HEAD^{tree}`; `git status --short`; `shasum -a 256` on the lead contract and the prior review; `python3 --version`; `sw_vers`; `uname -m`; the §4 script twice, the second stdout saved at the scratch path above. No cargo, cache, or device command.

Document SHA-256 is in the delivery message; it cannot be embedded here. The only new worktree file from this task is this note. The prior review remains the other untracked file and was not changed. Stop here for independent lead/Astra review.
