# H2b2 independent oracle review — 15 October 2026

**Verdict: Accept the frozen known-answer proposal for the two stated input domains.** No numerical, rounding, provenance, or claimed-witness defect found. This is a completed independent arithmetic review, not acceptance of an H2b2 implementation or an image comparison.

Reviewer: Codex CLI - Sol, slot `01a13e03-d91b-7810-a557-ca8e5fd5cd16`, actual Team roster model `gpt-6.1-sol`. Review snapshot: `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/oracle-review`, HEAD `8a4f02bcec8a3edb567979462cbf022adde6f8b9`. Environment observed: macOS 26.5.2 (25F84), arm64, Python 3.9.6. This review owns only this new Markdown file. Initial worktree status was clean.

## Inputs and exact identity

All SHA-256 values below were verified from bytes, with exit 0. Paths prefixed `lead/` and `harness-review/` are siblings of this snapshot under `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/`.

| Input | SHA-256 |
|---|---|
| `harness-review/docs/integration/H2B2-KNOWN-ANSWER-DESIGN-2026-10-15.md` | `3019ff22a4275b6ed18d0bbd434ea7c8482eac9c89fbb93d51d81f15569fa2b9` |
| `lead/docs/integration/H2-COMPARISON-CONTRACT-2026-10-15.md` | `f29936252b3ec103a41a76961d97902c82970416ea5802f88d497ef4341a6880` |
| `lead/docs/integration/H2B2-KNOWN-ANSWER-DRAFT-2026-10-15.md` | `22ab394a74f32af727e9b0058ee603c114346cbae039a3cb5d4f35b3f8a04ed2` |
| `/tmp/h2b2-oracle-20261015/derive.out` (4795 bytes) | `7bfa41f4cd4f6a09d117aa15abe9adbc389d675373dceace1e07f3ced3ded94f` |
| `/tmp/h2b2-oracle-20261015/amend.py` | `7a4a9ed7770439df9a226c3f3c92eb37ce14222b528cdc87a6130fff20f8e28d` |
| `/tmp/h2b2-oracle-20261015/amend.out` (4573 bytes) | `71c3cbbc9542fddce77031132bfccecf0d6c10b0505529c5c97be602c4b4d33a` |

Exact fence-payload hashes include the newline before the closing fence and exclude both fence delimiters:

| Payload | SHA-256 |
|---|---|
| Author §4 Python | `8b25f531bbdcf5649e409f2f610224b204b18644eceb47eb8eebbecc1312def9` |
| Author §4b Python | `7a4a9ed7770439df9a226c3f3c92eb37ce14222b528cdc87a6130fff20f8e28d` |
| Author §6.1 JSON | `869c2e15b4e8b4f8dacdb0f21029c546e4ee3cc29c6f6fec166de777b6c7974d` |
| Author §6.2 JSON | `146220417d246cbb01c3abe5250d4efeabc68fe5a5c59482f5c4b7a0c06d5403` |
| Independent review script below | `b5d41ca6df2616259896f2a1b892a09dc19667531705de9c4d2d18cbf9f47817` |
| Supplementary provenance script below | `fe20e5cf33ce6ae087b4c0a3a90c92430ef08034ea314a037073f847feef8a69` |

The second author fence equals `amend.py` byte for byte. Executing each inspected author fence with stdout captured only in memory reproduced its preserved output byte for byte. Those reruns are supplementary provenance checks; they are not the independent derivation. Parsing the prior frozen draft and the amendment also confirmed that the original eight sample records are unchanged.

## Independent derivation and primary references

The corrected sRGB/P3 provenance is sound. The official dated W3C source gives sRGB primaries in §10.2, unity transfer for linear sRGB in §10.3, and different red/green P3 primaries in §10.4. Its conversion examples give the rational sRGB matrix, white chromaticities, Bradford composition example, and piecewise Lab equations. The example code is nonnormative. [W3C CSS Color 4, 9 October 2026, §§10.2–10.4 and 19](https://www.w3.org/TR/2026/CRD-css-color-4-20261009/#color-conversion-code).

The reviewer independently constructed primary columns from chromaticities and solved their scale factors by Cramer's rule with exact `Fraction` arithmetic. The resulting nine matrix entries equal the official rational entries exactly. This uses neither the author's Gauss–Jordan/adjugate inverse nor any repository converter. The contract's Bradford cone matrix was then solved one column at a time with exact rational arithmetic. Exact checks give `M*[1,1,1] = W65`, `A*W65 = Wc`, and `A_reverse*A = I`.

The destination white was the exact decimal rational `[0.9642956764,1,0.8251046025]`, not the full xy quotient and not ICC nominal PCS white. Replacing it by the full quotient only for the primary-reference check reproduces the W3C printed adaptation matrix within `3.1641005281839613e-16`. The full-quotient white exceeds Wc by X `2.9567642956764296e-11`, Z `1.0460251046025105e-11`, Y zero. These independently match §5.

Nonlinear Lab terms were enclosed by **400 exact rational cube-root bisections**, not Newton iteration or decimal power. The interval endpoints satisfy the cube inequality. Every one of the 108 published XYZ/Lab components lies strictly within its published decimal's ±`5e-13` rounding cell; both endpoints also quantize to the published value. The largest component rounding-error upper bound is `4.947222901905558e-13`. Linear Lab terms, including every negative-blue component, remain rational. Neutral ratios reduce exactly to 0.18 and 2 before Lab, independently giving the gray and double-white closed forms. The exact join and negative linear-branch identity were checked.

The author's finite-precision residual table also reproduces: matrix/scale `2e-80`, inverse `7e-80`, adaptation `5e-80`, white `7e-80`, negative piecewise-equation residual `1e-80`, and gray closed form `49.49610761011958297899…`. These tiny decimal residuals are numerical execution details, not physical-white uncertainty.

## Values, precision domains, and witness coverage

**Pass — all 108 numeric components:** ten analytic rows × nine XYZ/Lab components, plus two f32 rows × nine. **Pass — all 43 published fault cells:** 35 adaptation/transpose/channel/RGB-clamp deltas and eight added clipping deltas. All match at 12 decimal places, without widening the strict absolute `<1e-6` tolerance.

The six f32 encodings were decoded independently from their IEEE-754 sign, exponent and significand bits into exact rational numbers. They equal all six exact widened-RGB strings. Neighbor comparisons also confirm the chosen f32 is nearest to each intended decimal. The Lab-domain gaps are:

| Sample | absolute L gap | absolute a gap | absolute b gap |
|---|---:|---:|---:|
| chroma | `4.808161004098024e-7` | `4.171875613489653e-7` | `1.245583826750867e-6` |
| extended | `5.420148903728160e-7` | `1.214485335944700e-6` | `1.732104521119809e-6` |

Thus §6.2 must be bound to its exact f32 bits and compared against its own expected values. Ideal decimal §6.1 is a different input domain. Decimal literals parsed into f64 are themselves rounded; independently substituting their exact binary64 values changed Lab by at most `4.188061371265599e-15` for this set. That harmless helper-input rounding does not justify treating f32 rounding as harmless, nor does it measure a future Rust implementation's arithmetic error. Gray18's analytic row likewise remains an analytic-helper row rather than an unlabelled pipeline golden.

Coverage and limits at author lines 583–625:

- Omitted, reversed and doubled adaptation, and both specifically defined transposes, are exposed by the nonblack witnesses reported in the table. The 35 table deltas match independently. Black is blind; white/gray are blind to RGB channel permutation.
- R↔B is exposed by primary red, chroma and extended. Additional reviewer probes found that the original eight collectively expose all five nonidentity RGB permutations, all five XYZ permutations before adaptation, and an accidental `M*A` instead of `A*M` composition. The smallest maximum witness gaps across those permutation sets are respectively `160.07598895083743` and `181.52442296590877`; the order swap has a maximum gap `15.65462552206170`. This is detection of these defined faults, not proof against every possible matrix defect.
- The original eight have nonnegative XYZ before and after adaptation and L in [0,100]; they cannot expose negative-XYZ or L-only clipping. Extended does expose RGB [0,1] clipping (`13.621693774145`).
- Negative blue has all-negative XYZ and L `-54.757649669435`. Either XYZ-negative clip and RGB clipping produce black, differing by `1253.472242328370` in maximum Lab component. L-only clipping differs by `54.757649669435`. The document correctly states that these outputs do not identify which of the three black-producing clips occurred.
- Double white retains positive XYZ and L `130.150841787805`. XYZ-negative clips do nothing; L-only and RGB clipping each differ by `30.150841787805` and are not separable at `1e-6`. The author's tiny residual distinguishing them is a finite-precision artifact, not a useful diagnostic.

No severity/location/input defect is raised. The explicit domain labels, unclipped expected values, and stated nonseparability must survive a later implementation.

## Executed checks, commands, and exclusions

All shell commands used the review snapshot as working directory unless a read targeted its sibling. Read-only identity/environment commands exited 0:

```sh
cd <HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/oracle-review
git rev-parse HEAD
git status --short
python3 --version
sw_vers
uname -m
shasum -a 256 <HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/harness-review/docs/integration/H2B2-KNOWN-ANSWER-DESIGN-2026-10-15.md <HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/lead/docs/integration/H2-COMPARISON-CONTRACT-2026-10-15.md /tmp/h2b2-oracle-20261015/derive.out /tmp/h2b2-oracle-20261015/amend.py /tmp/h2b2-oracle-20261015/amend.out
```

Read-only full text/line-number views of the proposal and contract were inspected. Official W3C §10.2–10.4 and conversion-code sections were opened with the web tool. The small arithmetic and provenance checks below were executed as `python3 - <<'PY'` heredocs, with each exact fence body followed by `PY`. Both final checks exited **0**. Earlier independent runs used the same arithmetic core; the final run adds exact rounding-cell assertions and the supplemental permutation probes. No failed arithmetic assertion occurred.

The final independent stdout was 4210 UTF-8 bytes, SHA-256 `70bb62a9eab2ab63f7a1dddd67e663824e616a88526161af762fea692b3bfc77`. It stayed in tool output/in-memory capture; no private output file or script was created. The full reproduction bodies below are the persistent review evidence. The report's own SHA-256 is supplied in the Team delivery message to avoid self-reference.

Report formatting check: `git diff --no-index --check /dev/null docs/integration/H2B2-ORACLE-INDEPENDENT-REVIEW-2026-10-15.md` initially exited 3 for one extra blank line at EOF. That report-only formatting issue was removed; the repeated check exited 0. Final `git status --short` lists only this new report. Embedded script payloads and frozen input hashes were rechecked unchanged.

**Pass:** frozen identities, primary-source check, exact algebra, all 108 values, six f32 input identities/nearest checks, 43 published fault cells, independent additional permutation/order probes, author-fence/output reproducibility, unchanged original eight records.

**Fail:** none found within this review.

**Not exercised:** Rust helper implementation, Rust/build/lint/tests, runtime JSON fixture loading, real candidate/RAW image processing, ICC parsing or PCS-D50→Wc reference conversion, reference custody, Adobe comparison or acceptance, GPU/Android/device routes, performance. No shared binary or cache was used. No production converter generated an expected value. No binary was produced, so there is no review binary hash.

Only this new report is written; no implementation, fixture JSON, dependency/lock, ICC, device, Git metadata, or other worker file was changed. This completes task `01a13e55-3710-7be1-a923-6278fba88448` and stops at review, with integration/acceptance retained by the lead.

## Independent arithmetic reproduction

```python
from pathlib import Path
from fractions import Fraction as F
from decimal import Decimal as D, localcontext, ROUND_HALF_EVEN
import re, json, struct, hashlib, io, contextlib, platform, sys

ROOT = Path('<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015')
p = ROOT/'harness-review/docs/integration/H2B2-KNOWN-ANSWER-DESIGN-2026-10-15.md'
doc = p.read_text()
datasets = [json.loads(s, parse_float=D) for s in re.findall(r'\x60\x60\x60json\n(.*?)\x60\x60\x60', doc, re.S)]
assert len(datasets) == 2
analytic, widened = datasets
Q = D('1e-12')
def dec(x):
    x = F(x)
    with localcontext() as c:
        c.prec = 130
        return D(x.numerator)/D(x.denominator)
def quant(x):
    with localcontext() as c:
        c.prec = 130
        return x.quantize(Q, rounding=ROUND_HALF_EVEN)
def det(m):
    return (m[0][0]*m[1][1]*m[2][2]+m[0][1]*m[1][2]*m[2][0]+m[0][2]*m[1][0]*m[2][1]
           -m[0][2]*m[1][1]*m[2][0]-m[0][1]*m[1][0]*m[2][2]-m[0][0]*m[1][2]*m[2][1])
def solve(m,b):
    den = det(m)
    return tuple(det(tuple(tuple(b[r] if c == col else m[r][c] for c in range(3)) for r in range(3)))/den for col in range(3))
def mv(m,v):
    return tuple(sum((m[r][c]*v[c] for c in range(3)),F(0)) for r in range(3))
def mm(a,b):
    return tuple(tuple(sum((a[r][k]*b[k][c] for k in range(3)),F(0)) for c in range(3)) for r in range(3))
def tr(m):
    return tuple(zip(*m))
def xy(x,y):
    x,y=F(x),F(y)
    return x/y,F(1),(1-x-y)/y
W65=xy('0.3127','0.3290')
Wc=tuple(map(F,('0.9642956764','1','0.8251046025')))
prim=tuple(xy(x,y) for x,y in [('0.64','0.33'),('0.30','0.60'),('0.15','0.06')])
P=tr(prim)
scales=solve(P,W65)
M=tuple(tuple(P[r][c]*scales[c] for c in range(3)) for r in range(3))
official=((F(506752,1228815),F(87881,245763),F(12673,70218)),
          (F(87098,409605),F(175762,245763),F(12673,175545)),
          (F(7918,409605),F(87881,737289),F(1001167,1053270)))
assert M == official
B=tuple(tuple(map(F,row.split())) for row in ['.8951 .2664 -.1614','-.7502 1.7135 .0367','.0389 -.0685 1.0296'])
def adapt(src,dst):
    a,b=mv(B,src),mv(B,dst)
    cols=[solve(B,tuple(B[r][col]*b[r]/a[r] for r in range(3))) for col in range(3)]
    return tr(cols)
A=adapt(W65,Wc)
rev=adapt(Wc,W65)
assert mv(M,(F(1),)*3)==W65
assert mv(A,W65)==Wc
assert mm(rev,A)==tuple(tuple(F(r==c) for c in range(3)) for r in range(3))

# Exact rational bisection: enclosing endpoints, never a Newton/power approximation.
def cubebounds(t):
    assert t > 0
    lo,hi=F(0),max(F(1),t)
    for _ in range(400):
        mid=(lo+hi)/2
        if mid**3 <= t: lo=mid
        else: hi=mid
    assert lo**3 <= t <= hi**3
    return lo,hi
eps=F(216,24389)
kappa=F(24389,27)
def fbounds(t):
    if t<=eps:
        exact=(kappa*t+16)/116
        assert exact == t/(3*F(6,29)**2)+F(4,29)
        return exact,exact
    return cubebounds(t)
def labbounds(xyz):
    fx,fy,fz=[fbounds(xyz[i]/Wc[i]) for i in range(3)]
    return ((116*fy[0]-16,116*fy[1]-16),
            (500*(fx[0]-fy[1]),500*(fx[1]-fy[0])),
            (200*(fy[0]-fz[1]),200*(fy[1]-fz[0])))
def lab(xyz):
    bounds=labbounds(xyz)
    assert all(hi-lo<F('1e-110') for lo,hi in bounds)
    return tuple((lo+hi)/2 for lo,hi in bounds)
def correct(rgb):
    xyz=mv(M,rgb)
    wc=mv(A,xyz)
    return xyz,wc,labbounds(wc)
def scalar_bounds(x):
    return (x,x)
errors=[]
component_count=0
results={}
def check_bounds(bounds,expected,label):
    global component_count
    lo,hi=bounds
    assert quant(dec(lo))==D(expected)==quant(dec(hi)),(label,dec(lo),expected,dec(hi))
    e=F(expected)
    assert e-F('5e-13') < lo <= hi < e+F('5e-13')
    errors.append(max(abs(lo-e),abs(hi-e)))
    component_count+=1
for sample in analytic['samples']:
    rgb=tuple(map(F,sample['rgb']))
    xyz,wc,bounds=correct(rgb)
    for field,values in [('xyz_d65',tuple(map(scalar_bounds,xyz))),('xyz_wc',tuple(map(scalar_bounds,wc))),('lab',bounds)]:
        for i,(v,e) in enumerate(zip(values,sample[field])):
            check_bounds(v,e,(sample['name'],field,i))
    results[sample['name']]={'rgb':rgb,'lab':tuple((lo+hi)/2 for lo,hi in bounds)}
# Decode IEEE754 bits directly, not through a float-to-Decimal oracle.
def f32bits(h):
    u=int.from_bytes(bytes.fromhex(h),'little')
    sign=-1 if u>>31 else 1
    exponent=(u>>23)&255
    fraction=u&((1<<23)-1)
    assert exponent!=255
    significand=fraction if exponent==0 else (1<<23)+fraction
    power=-149 if exponent==0 else exponent-150
    return sign*F(significand)*(F(2)**power)
gaps={}
for sample in widened['samples']:
    rgb=tuple(map(f32bits,sample['f32_le_hex']))
    assert rgb==tuple(map(F,sample['rgb_exact']))
    ideal=results[sample['name']]['rgb']
    for target,h in zip(ideal,sample['f32_le_hex']):
        assert struct.pack('<f',float(target)).hex()==h
        u=int.from_bytes(bytes.fromhex(h),'little')
        candidate=f32bits(h)
        neighbors=[f32bits((u+offset).to_bytes(4,'little').hex()) for offset in (-1,1)]
        assert all(abs(candidate-target)<abs(n-target) for n in neighbors)
    xyz,wc,bounds=correct(rgb)
    for field,values in [('xyz_d65',tuple(map(scalar_bounds,xyz))),('xyz_wc',tuple(map(scalar_bounds,wc))),('lab',bounds)]:
        for i,(v,e) in enumerate(zip(values,sample[field])):
            check_bounds(v,e,(sample['name'],'f32',field,i))
    actual=tuple((lo+hi)/2 for lo,hi in bounds)
    gap=tuple(abs(x-y) for x,y in zip(actual,results[sample['name']]['lab']))
    gaps[sample['name']]=[str(dec(x)) for x in gap]
assert component_count==108
# f64 parsing of decimal helper literals remains well inside analytic tolerance.
f64_gap=F(0)
for sample in analytic['samples']:
    widened64=tuple(F.from_float(float(x)) for x in sample['rgb'])
    actual=lab(mv(A,mv(M,widened64)))
    f64_gap=max(f64_gap,max(abs(x-y) for x,y in zip(actual,results[sample['name']]['lab'])))
assert f64_gap<F('1e-6')

# Independently recompute every published fault-table cell from the defined faults.
def faultlabs(rgb):
    xyz=mv(M,rgb)
    wc=mv(A,xyz)
    return [lab(xyz), lab(mv(rev,xyz)),lab(mv(A,wc)),
            lab(mv(A,mv(tr(M),rgb))),lab(mv(tr(A),xyz)),
            lab(mv(A,mv(M,rgb[::-1]))),
            lab(mv(A,mv(M,tuple(min(F(1),max(F(0),x)) for x in rgb))))]
fault_cells=0
fault_deltas={}
for name in ['white','gray18','primary_r','chroma','extended']:
    row=re.search(r'^\| '+name+r' \| ([0-9. \|]+)\|$',doc,re.M).group(1)
    expected=[D(s.strip()) for s in row.split('|') if s.strip()]
    assert len(expected)==7
    ds=[max(abs(x-y) for x,y in zip(out,results[name]['lab'])) for out in faultlabs(results[name]['rgb'])]
    assert list(map(lambda x:quant(dec(x)),ds))==expected,(name,ds,expected)
    assert all(abs(value-F(want))<F('5e-13')-F('2e-110') for value,want in zip(ds,expected))
    fault_deltas[name]=list(map(lambda x:str(quant(dec(x))),ds))
    fault_cells+=7
clip_cells=0
clip_deltas={}
for name in ['negative_blue','double_white']:
    rgb=results[name]['rgb']
    xyz=mv(M,rgb);wc=mv(A,xyz);base=results[name]['lab']
    outs=[lab(mv(A,tuple(max(F(0),x) for x in xyz))),
          lab(tuple(max(F(0),x) for x in wc)),
          (min(F(100),max(F(0),base[0])),base[1],base[2]),
          faultlabs(rgb)[-1]]
    ds=[max(abs(x-y) for x,y in zip(out,base)) for out in outs]
    row=re.search(r'^\| '+name+r' \| ([0-9. \|]+)\|$',doc,re.M).group(1)
    expected=[D(s.strip()) for s in row.split('|') if s.strip()]
    assert list(map(lambda x:quant(dec(x)),ds))==expected,(name,ds,expected)
    assert all(abs(value-F(want))<F('5e-13')-F('2e-110') for value,want in zip(ds,expected))
    clip_deltas[name]=list(map(lambda x:str(quant(dec(x))),ds))
    clip_cells+=4
# All eight originals are blind to the two XYZ-negative clips and L-only clipping.
for sample in analytic['samples'][:8]:
    xyz,wc,bounds=correct(tuple(map(F,sample['rgb'])))
    assert min(xyz)>=0 and min(wc)>=0
    assert F(0)<=bounds[0][0]<=bounds[0][1]<=F(100)

# Closed-form checks: exact neutral ratios, and exact rational negative-branch Lab.
assert mv(A,mv(M,(F('.18'),)*3))==tuple(F('.18')*w for w in Wc)
assert mv(A,mv(M,(F(2),)*3))==tuple(2*w for w in Wc)
nb=mv(A,mv(M,(F(0),F(0),F(-1))))
ratio=tuple(nb[i]/Wc[i] for i in range(3))
closed_negative=(kappa*ratio[1],F(500,116)*kappa*(ratio[0]-ratio[1]),F(200,116)*kappa*(ratio[1]-ratio[2]))
assert closed_negative==results['negative_blue']['lab']
assert all(fbounds(t)==(F(6,29),F(6,29)) for t in [eps])
assert fbounds(F('-0.02'))==(F('-0.02')/(3*F(6,29)**2)+F(4,29),)*2

W50=xy('.3457','.3585')
full=adapt(W65,W50)
printed=tuple(tuple(map(F,row.split())) for row in [
'1.0479297925449969 .022946870601609652 -.05019226628920524',
'.02962780877005599 .9904344267538799 -.017073799063418826',
'-.009243040646204504 .015055191490298152 .7518742814281371'])
primary_delta=max(abs(full[r][c]-printed[r][c]) for r in range(3) for c in range(3))
assert primary_delta<F('4e-16')
print(json.dumps({
 'python':sys.version.split()[0],
 'matrix_equals_primary_rationals':True,'white_mapping_exact':True,'reverse_times_forward_exact_identity':True,
 'numeric_fixture_components':component_count,
 'maximum_component_rounding_error_upper_bound':str(dec(max(errors))),
 'fault_cells_checked':fault_cells,'clip_cells_checked':clip_cells,
 'f32_exact_components_and_nearest_checks':6,
 'f32_Lab_gaps_L_a_b':gaps,
 'f64_literal_max_Lab_gap':str(dec(f64_gap)),
 'full_D50_minus_Wc':[str(dec(W50[i]-Wc[i])) for i in range(3)],
 'W3C_printed_matrix_max_gap':str(dec(primary_delta)),
 'fault_table':fault_deltas,'clip_table':clip_deltas,
 'analytic_rgb_json_sha256':hashlib.sha256(re.findall(r'\x60\x60\x60json\n(.*?)\x60\x60\x60',doc,re.S)[0].encode()).hexdigest(),
 'f32_json_sha256':hashlib.sha256(re.findall(r'\x60\x60\x60json\n(.*?)\x60\x60\x60',doc,re.S)[1].encode()).hexdigest()
},indent=2))

from itertools import permutations
nonidentity=[p for p in permutations(range(3)) if p!=(0,1,2)]
def perm(v,p):
    return tuple(v[i] for i in p)
original=analytic['samples'][:8]
def largest_fault_gap(transform):
    best=F(0)
    for sample in original:
        rgb=tuple(map(F,sample['rgb']))
        out=transform(rgb)
        base=results[sample['name']]['lab']
        best=max(best,max(abs(x-y) for x,y in zip(out,base)))
    return best
rgb_perm=[largest_fault_gap(lambda rgb,p=p:lab(mv(A,mv(M,perm(rgb,p))))) for p in nonidentity]
xyz_perm=[largest_fault_gap(lambda rgb,p=p:lab(mv(A,perm(mv(M,rgb),p)))) for p in nonidentity]
order_swap=largest_fault_gap(lambda rgb:lab(mv(M,mv(A,rgb))))
assert min(rgb_perm)>F('1e-6') and min(xyz_perm)>F('1e-6') and order_swap>F('1e-6')
print(json.dumps({
 'extra_all_five_nonidentity_RGB_permutations_detected':True,
 'extra_all_five_nonidentity_XYZ_permutations_detected':True,
 'extra_M_A_order_swap_detected':True,
 'smallest_across_RGB_permutations_max_witness_gap':str(dec(min(rgb_perm))),
 'smallest_across_XYZ_permutations_max_witness_gap':str(dec(min(xyz_perm))),
 'M_A_order_swap_max_witness_gap':str(dec(order_swap))
},indent=2))
```

## Supplementary author-fence/provenance reproduction

```python
from pathlib import Path
import re, hashlib, contextlib, io, json
root=Path('<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015')
path=root/'harness-review/docs/integration/H2B2-KNOWN-ANSWER-DESIGN-2026-10-15.md'
text=path.read_text()
scripts=re.findall(r'\x60\x60\x60python\n(.*?)\x60\x60\x60',text,re.S)
assert len(scripts)==2
base=Path('/tmp/h2b2-oracle-20261015')
print('script_fence_hashes',[hashlib.sha256(x.encode()).hexdigest() for x in scripts])
assert scripts[1].encode()==(base/'amend.py').read_bytes()
for script,name in zip(scripts,['derive.out','amend.out']):
    out=io.StringIO()
    with contextlib.redirect_stdout(out):
        exec(compile(script,'<author-fence>','exec'),{})
    actual=out.getvalue().encode()
    expected=(base/name).read_bytes()
    assert actual==expected,(name,len(actual),len(expected))
    print(name,'exact stdout match',len(actual),hashlib.sha256(actual).hexdigest())
    if name=='derive.out':
        print('derive_crosschecks')
        for line in actual.decode().splitlines():
            if not line.startswith(('  ','white omit','chroma omit','extended omit','primary_r omit','gray18 omit')):
                print(line)
old=root/'lead/docs/integration/H2B2-KNOWN-ANSWER-DRAFT-2026-10-15.md'
assert hashlib.sha256(old.read_bytes()).hexdigest()=='22ab394a74f32af727e9b0058ee603c114346cbae039a3cb5d4f35b3f8a04ed2'
old_json=[json.loads(s) for s in re.findall(r'\x60\x60\x60json\n(.*?)\x60\x60\x60',old.read_text(),re.S)]
new_json=[json.loads(s) for s in re.findall(r'\x60\x60\x60json\n(.*?)\x60\x60\x60',text,re.S)]
assert old_json[0]['samples']==new_json[0]['samples'][:8]
print('original_eight_match_frozen_draft',True)
```

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `7d64cd842c6deb2349bb7e59a407e5759fd5b0c4a323e338b971b17fdaf6fbe8` (24586 bytes).
