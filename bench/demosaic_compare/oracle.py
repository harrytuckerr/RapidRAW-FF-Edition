"""Frozen X3 array oracle. No rendering, decoding, fitting candidates or I/O.

Inputs are caller-decoded, oriented and aligned arrays. Context/acceptance hashes
are evidence bindings, not authentication or proof that the caller's claims are
true. No test result here establishes real-frame LibRaw or GPU parity.
"""
from dataclasses import asdict, dataclass
from hashlib import sha256
import json
import math
import re

import numpy as np
import scipy
from scipy import ndimage

NUMPY_VERSION = '2.2.6'
SCIPY_VERSION = '1.15.3'
CAMERA = 'camera-linear-white-one'
LINEAR = 'srgb-linear-white-one'
ENCODED = 'srgb-encoded-unit'
CROPS = (
    ('DSCF2095', ((3180, 2088), (3660, 1615), (3518, 393), (2200, 2400), (2888, 1425))),
    ('DSCF1895', ((2450, 318), (2165, 1142), (2219, 1923), (2279, 1473), (2501, 786))),
)
CROP_SIZE = 140


class OracleError(ValueError):
    """Invalid or incomparable evidence; never interpreted as a passing score."""


def _require(condition, message):
    if not condition:
        raise OracleError(message)


def _finite(value, positive=False):
    _require(type(value) in (int, float) and math.isfinite(value), 'finite scalar required')
    _require(not positive or value > 0, 'positive scalar required')


def _digest(value):
    _require(type(value) is str and re.fullmatch('[0-9a-f]{64}', value) is not None, 'SHA256 required')


def _json(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False)


def _hash(value):
    return sha256(_json(value).encode()).hexdigest()


def _versions():
    _require(np.__version__ == NUMPY_VERSION and scipy.__version__ == SCIPY_VERSION,
             'pinned NumPy/SciPy required for quantiles and filters')


def _array(value):
    _require(type(value) is np.ndarray, 'ndarray required')
    _require(value.ndim == 3 and value.shape[2] == 3 and min(value.shape[:2]) > 0,
             'nonempty HxWx3 required')
    _require(value.dtype in (np.dtype('float32'), np.dtype('float64')), 'float32/float64 required')
    _require(bool(np.isfinite(value).all()), 'nonfinite samples')
    return value.astype(np.float64, copy=False)


@dataclass(frozen=True, slots=True, init=False, eq=False)
class Image:
    """Immutable byte-backed float64 snapshot; declared input dtype remains hashed."""
    _buffer: bytes
    shape: tuple
    dtype: str
    units: str

    def __init__(self, pixels, *, dtype, units):
        _require(dtype in ('float32', 'float64'), 'explicit float dtype required')
        a = _array(pixels)
        _require(pixels.dtype == np.dtype(dtype), 'declared dtype mismatch')
        _require(units in (CAMERA, LINEAR, ENCODED), 'explicit units required')
        if units == ENCODED:
            _require(bool(((a >= 0) & (a <= 1)).all()), 'encoded samples outside unit interval')
        object.__setattr__(self, '_buffer', a.astype('<f8').tobytes(order='C'))
        object.__setattr__(self, 'shape', a.shape)
        object.__setattr__(self, 'dtype', dtype)
        object.__setattr__(self, 'units', units)

    @property
    def pixels(self):
        return np.frombuffer(self._buffer, dtype='<f8').reshape(self.shape)

    @property
    def digest(self):
        header = _json({'shape':self.shape, 'dtype':self.dtype, 'units':self.units}).encode()
        return sha256(header + b'\0' + self._buffer).hexdigest()


def _image(pixels, units):
    return Image(pixels, dtype='float64', units=units)


@dataclass(frozen=True, slots=True)
class Geometry:
    height: int
    width: int
    source_height: int
    source_width: int
    origin_y: int
    origin_x: int
    orientation: int
    alignment_sha256: str
    seam_rows: tuple
    seam_cols: tuple

    def __post_init__(self):
        for n in (self.height, self.width, self.source_height, self.source_width):
            _require(type(n) is int and n > 0, 'positive geometry required')
        for n in (self.origin_y, self.origin_x):
            _require(type(n) is int and n >= 0, 'nonnegative ROI origin required')
        _require(self.origin_y + self.height <= self.source_height and
                 self.origin_x + self.width <= self.source_width, 'ROI outside source')
        _require(type(self.orientation) is int and 0 <= self.orientation <= 3, 'quarter-turn metadata required')
        _digest(self.alignment_sha256)
        for seams, size in ((self.seam_rows, self.height), (self.seam_cols, self.width)):
            _require(type(seams) is tuple and all(type(n) is int and 0 < n < size for n in seams), 'invalid seams')
            _require(tuple(sorted(set(seams))) == seams, 'seams must be unique ordered output coordinates')


@dataclass(frozen=True, slots=True)
class Transform:
    camera_to_srgb: tuple
    wb: tuple
    exposure: float
    reference_sha256: str
    acceptance_sha256: str

    def __post_init__(self):
        _require(type(self.camera_to_srgb) is tuple and len(self.camera_to_srgb) == 3 and
                 all(type(row) is tuple and len(row) == 3 for row in self.camera_to_srgb), 'immutable 3x3 matrix required')
        for row in self.camera_to_srgb:
            for x in row:
                _finite(x)
        _require(type(self.wb) is tuple and len(self.wb) == 3, 'immutable RGB WB required')
        for x in self.wb:
            _finite(x, True)
        _finite(self.exposure, True)
        _digest(self.reference_sha256)
        _digest(self.acceptance_sha256)
        a = np.array(self.camera_to_srgb, dtype=np.float64)
        try:
            inv = np.linalg.inv(a)
        except np.linalg.LinAlgError as exc:
            raise OracleError('singular matrix') from exc
        _require(bool(np.isfinite(inv).all()) and float(np.max(np.abs(a @ inv - np.eye(3)))) <= 1e-4,
                 'invalid matrix inverse')
        effective_wb(self.wb)


@dataclass(frozen=True, slots=True)
class Context:
    frame: str
    source_sha256: str
    passes: int
    geometry: Geometry
    transform: Transform

    def __post_init__(self):
        _require(type(self.frame) is str and 0 < len(self.frame) <= 128, 'frame identity required')
        _digest(self.source_sha256)
        _require(type(self.passes) is int and self.passes in (1, 3), 'same pass 1 or 3 required')
        _require(type(self.geometry) is Geometry and type(self.transform) is Transform, 'typed immutable context required')

    @property
    def digest(self):
        return _hash(asdict(self))


def effective_wb(values):
    """Legacy missing R-WB convention; unused RGBE fourth slot is ignored."""
    _require(type(values) is tuple and len(values) in (3, 4), 'RGB/RGBE WB tuple required')
    rgb = values[:3]
    _require(all(type(x) in (int, float) for x in rgb), 'numeric WB required')
    if math.isnan(rgb[0]):
        return (1.0, 1.0, 1.0)
    for x in rgb:
        _finite(x, True)
    result = tuple(x / rgb[1] for x in rgb)
    for x in result:
        _finite(x, True)
    return result


def interpolation_balance(image, scale, wb, *, undo=False):
    _require(type(image) is Image and image.units == CAMERA, 'camera units required')
    _finite(scale, True)
    _require(type(undo) is bool, 'explicit undo flag required')
    weights = np.array(effective_wb(wb), dtype=np.float64) * scale
    _require(bool(np.isfinite(weights).all()) and bool((weights > 0).all()), 'WB/scale overflow')
    with np.errstate(over='ignore', invalid='ignore', divide='ignore'):
        result = image.pixels / weights if undo else image.pixels * weights
    return _image(result, CAMERA)


def linear_srgb(image, transform):
    _require(type(image) is Image and image.units == CAMERA and type(transform) is Transform, 'camera transform required')
    with np.errstate(over='ignore', invalid='ignore'):
        result = (image.pixels * effective_wb(transform.wb)) @ np.array(transform.camera_to_srgb).T
        result *= transform.exposure
    return _image(result, LINEAR)


def encode_srgb(image):
    """Display-only clipping; original signed/HDR linear samples remain available."""
    _require(type(image) is Image and image.units == LINEAR, 'linear sRGB required')
    a = image.pixels
    counts = {'below_zero':int(np.count_nonzero(a < 0)), 'above_one':int(np.count_nonzero(a > 1))}
    a = np.clip(a, 0.0, 1.0)
    result = np.empty_like(a)
    low = a <= 0.0031308
    result[low] = 12.92 * a[low]
    result[~low] = 1.055 * np.power(a[~low], 1.0 / 2.4) - 0.055
    # pow(1) roundoff may lie infinitesimally outside the display interval.
    return _image(np.clip(result, 0.0, 1.0), ENCODED), counts


def quantize_srgb(image):
    _require(type(image) is Image and image.units == ENCODED, 'encoded sRGB required')
    return np.floor(255.0 * image.pixels + 0.5).astype(np.float64)


def _stats(errors):
    flat = np.asarray(errors, dtype=np.float64).ravel()
    _require(flat.size > 0 and bool(np.isfinite(flat).all()) and bool((flat >= 0).all()), 'invalid error population')
    # Integer ceil avoids both floating percentile indices and interpolated ranks.
    index = (99 * flat.size + 99) // 100 - 1
    with np.errstate(over='ignore'):
        mean = float(np.sum(flat, dtype=np.float64) / flat.size)
    _finite(mean)
    return {'channels':int(flat.size), 'mean':mean, 'p99':float(np.partition(flat, index)[index]), 'max':float(np.max(flat))}


def _regions(errors, geometry):
    h, w = geometry.height, geometry.width
    yy, xx = np.ogrid[:h, :w]
    border = (yy < 8) | (yy >= h - 8) | (xx < 8) | (xx >= w - 8)
    seam = np.zeros((h, w), dtype=bool)
    for row in geometry.seam_rows:
        seam[max(0,row-8):min(h,row+8), :] = True
    for col in geometry.seam_cols:
        seam[:, max(0,col-8):min(w,col+8)] = True
    # Diagnostic border/seam may overlap; whole always includes each channel once.
    regions = {'border8':border, 'seam16':seam, 'interior':~(border | seam)}
    return {name: {'channels':int(mask.sum()) * 3, 'stats':_stats(errors[mask]) if mask.any() else None}
            for name, mask in regions.items()}


def _comparison(candidate, reference, context, candidate_context):
    _require(type(context) is Context and type(candidate_context) is Context and context == candidate_context,
             'context mismatch; no candidate fitting or alignment')
    for image in (candidate, reference):
        _require(type(image) is Image and image.units == CAMERA, 'camera-white-one input required')
        _require(image.pixels.shape == (context.geometry.height, context.geometry.width, 3), 'geometry mismatch')
    _require(reference.digest == context.transform.reference_sha256, 'fixed transform reference mismatch')
    result = {}
    camera_error = np.abs(candidate.pixels - reference.pixels)
    c_lin, r_lin = linear_srgb(candidate, context.transform), linear_srgb(reference, context.transform)
    c_enc, c_clip = encode_srgb(c_lin)
    r_enc, r_clip = encode_srgb(r_lin)
    errors = {'linear_camera':camera_error, 'linear_srgb':np.abs(c_lin.pixels-r_lin.pixels),
              'display_float':np.abs(c_enc.pixels-r_enc.pixels)*255.0,
              'display_u8':np.abs(quantize_srgb(c_enc)-quantize_srgb(r_enc))}
    for space, err in errors.items():
        result[space] = {'whole':_stats(err), 'regions':_regions(err, context.geometry)}
    return {'context_sha256':context.digest, 'candidate_sha256':candidate.digest,
            'reference_sha256':reference.digest, 'excluded_channels':0,
            'clipped':{'candidate':c_clip, 'reference':r_clip}, 'errors':result}


def compare_cpu(candidate, reference, context, candidate_context):
    result = _comparison(candidate, reference, context, candidate_context)
    result['passed'] = result['errors']['display_u8']['whole']['mean'] <= 0.5
    result['gate'] = 'same-pass-LibRaw-mean-u8<=0.5; reference provenance must be verified by caller'
    return result


def compare_gpu(candidate, reference, context, candidate_context):
    result = _comparison(candidate, reference, context, candidate_context)
    limits = {'linear_camera':(1e-5,1e-4,1e-3), 'linear_srgb':(1e-5,1e-4,1e-3),
              'display_float':(0.02,0.10,0.50)}
    result['space_passed'] = {s:all(result['errors'][s]['whole'][k] <= v for k,v in zip(('mean','p99','max'),lim))
                              for s,lim in limits.items()}
    result['passed'] = all(result['space_passed'].values())
    return result


def colour_metrics(encoded):
    _versions()
    _require(type(encoded) is Image and encoded.units == ENCODED, 'encoded sRGB required')
    y, cb, cr = _ycc(encoded.pixels)
    cb = cb - ndimage.uniform_filter(cb, size=5, mode='reflect', output=np.float64)
    cr = cr - ndimage.uniform_filter(cr, size=5, mode='reflect', output=np.float64)
    grad = np.hypot(ndimage.sobel(y, axis=0, mode='reflect', output=np.float64),
                    ndimage.sobel(y, axis=1, mode='reflect', output=np.float64))
    return {'hf_chroma_rms':float(255.0*np.sqrt(np.mean(cb*cb+cr*cr,dtype=np.float64))),
            'luma_gradient':float(255.0*np.mean(grad,dtype=np.float64))}


def _ycc(a):
    y = 0.2126*a[...,0]+0.7152*a[...,1]+0.0722*a[...,2]
    return y, a[...,2]-y, a[...,0]-y


def gap_gate(baseline, neutral, filtered):
    keys = {'hf_chroma_rms','luma_gradient'}
    for m in (baseline,neutral,filtered):
        _require(type(m) is dict and set(m)==keys, 'closed metric record required')
        for x in m.values():
            _finite(x)
            _require(x >= 0, 'negative metric')
    m,n,f = (x['hf_chroma_rms'] for x in (baseline,neutral,filtered))
    limit = n+0.5*(m-n) if m>n else m
    sharp = 0.97*baseline['luma_gradient']
    return {'passed':f<=limit and filtered['luma_gradient']>=sharp,
            'chroma_limit':limit,'minimum_sharpness':sharp,'positive_gap':m>n}


def frame_gap_gates(frame, baseline, neutral, filtered):
    coords = dict(CROPS).get(frame)
    _require(coords is not None, 'unknown frame')
    _require(all(type(x) in (list,tuple) and len(x)==5 for x in (baseline,neutral,filtered)), 'all five crops required')
    rows=[gap_gate(m,n,f) for m,n,f in zip(baseline,neutral,filtered)]
    means=[{k:math.fsum(x[k] for x in values)/5 for k in ('hf_chroma_rms','luma_gradient')}
           for values in (baseline,neutral,filtered)]
    overall=gap_gate(*means)
    return {'frame':frame,'coordinates':coords,'crops':rows,'frame_means':overall,
            'passed':all(row['passed'] for row in rows) and overall['passed']}


def both_frames_gate(results):
    _require(type(results) in (list,tuple) and len(results)==2, 'both frame reports required')
    _require({r['frame'] for r in results}=={x[0] for x in CROPS}, 'exact frame pair required')
    return all(r['passed'] is True for r in results)


@dataclass(frozen=True, slots=True)
class CropPlan:
    """Inputs already oriented; neutral coordinates are (y-dy,x-dx).

    Offsets are supplied/frozen externally, never searched by this library.
    No default shifts: absent historical per-crop registration is not guessed.
    """
    neutral_height: int
    neutral_width: int
    neutral_rotation: int
    offsets_yx: tuple
    alignment_sha256: str

    def __post_init__(self):
        _require(all(type(n) is int and n>0 for n in (self.neutral_height,self.neutral_width)), 'neutral extent required')
        _require(type(self.neutral_rotation) is int and 0<=self.neutral_rotation<=3, 'neutral rotation required')
        _require(type(self.offsets_yx) is tuple and len(self.offsets_yx)==5 and
                 all(type(t) is tuple and len(t)==2 and all(type(n) is int for n in t) for t in self.offsets_yx),
                 'five explicit immutable offsets required')
        _digest(self.alignment_sha256)


def mapping_context(context, plan):
    _require(type(context) is Context and context.passes==1 and type(plan) is CropPlan,
             'accepted CPU1 context and crop plan required')
    _require(context.frame in dict(CROPS), 'historical frame required')
    return _hash({'context':asdict(context),'crop_plan':asdict(plan)})


@dataclass(frozen=True, slots=True)
class MapAuthority:
    """Externally accepted unsuppressed CPU1 / C1-neutral input commitments.

    These are not generated from a candidate by the scorer. Supplying a new
    acceptance/hash is a new protocol decision, not automatic authorization.
    """
    baseline_sha256: str
    neutral_sha256: str
    context_sha256: str
    acceptance_sha256: str

    def __post_init__(self):
        for value in asdict(self).values():
            _digest(value)


@dataclass(frozen=True, slots=True)
class ToneMap:
    authority: MapAuthority
    knots: tuple

    def __post_init__(self):
        _require(type(self.authority) is MapAuthority, 'map authority required')
        _require(type(self.knots) is tuple and len(self.knots)==3, 'three channel maps required')
        for pair in self.knots:
            _require(type(pair) is tuple and len(pair)==2, 'source/target knots required')
            for values in pair:
                _require(type(values) is tuple and len(values)==1025, '1025 immutable quantiles required')
                for value in values:
                    _finite(value)
                    _require(0<=value<=1, 'encoded quantile outside unit interval')
                _require(all(a<=b for a,b in zip(values,values[1:])), 'nonmonotone quantiles')

    def to_json(self):
        return _json({'schemaVersion':1,'authority':asdict(self.authority),'knots':self.knots,
                      'seed':0,'sampleLimit':2_000_000,'quantileMethod':'linear',
                      'duplicateKnots':'numpy.interp-rightmost','numpy':NUMPY_VERSION,'scipy':SCIPY_VERSION})

    @property
    def digest(self):
        return sha256(self.to_json().encode()).hexdigest()

    @classmethod
    def from_json(cls, text, *, expected_sha256):
        _digest(expected_sha256)
        _require(type(text) is str and len(text.encode())<=1_048_576, 'bounded map JSON required')
        _require(sha256(text.encode()).hexdigest()==expected_sha256, 'saved map hash mismatch')
        def pairs(items):
            d={}
            for k,v in items:
                _require(k not in d, 'duplicate map field')
                d[k]=v
            return d
        try:
            d=json.loads(text,object_pairs_hook=pairs,parse_constant=lambda _: (_ for _ in ()).throw(OracleError('nonfinite JSON')))
            _require(type(d) is dict and set(d)=={'schemaVersion','authority','knots','seed','sampleLimit','quantileMethod','duplicateKnots','numpy','scipy'}, 'closed map schema required')
            _require(type(d['schemaVersion']) is int and d['schemaVersion']==1 and type(d['seed']) is int and d['seed']==0 and
                     type(d['sampleLimit']) is int and d['sampleLimit']==2_000_000 and d['quantileMethod']=='linear' and
                     d['duplicateKnots']=='numpy.interp-rightmost' and d['numpy']==NUMPY_VERSION and d['scipy']==SCIPY_VERSION,
                     'map policy/version mismatch')
            _require(type(d['authority']) is dict and set(d['authority'])=={'baseline_sha256','neutral_sha256','context_sha256','acceptance_sha256'}, 'closed map authority required')
            result=cls(MapAuthority(**d['authority']),tuple(tuple(tuple(v) for v in pair) for pair in d['knots']))
        except (TypeError,KeyError,json.JSONDecodeError) as exc:
            raise OracleError('malformed saved map') from exc
        _require(result.to_json()==text, 'canonical saved map required')
        return result


def build_reference_map(baseline, neutral, context, plan, authority):
    """Only committed reference inputs; no candidate argument or fitting in scoring."""
    _versions()
    _require(type(authority) is MapAuthority, 'frozen acceptance authority required')
    _require(type(baseline) is Image and type(neutral) is Image and
             baseline.units==neutral.units==ENCODED, 'encoded reference images required')
    _require(baseline.digest==authority.baseline_sha256 and neutral.digest==authority.neutral_sha256,
             'input is not the frozen accepted baseline/neutral')
    _require(mapping_context(context,plan)==authority.context_sha256, 'map context mismatch')
    _require(baseline.pixels.shape==(context.geometry.height,context.geometry.width,3) and
             neutral.pixels.shape==(plan.neutral_height,plan.neutral_width,3), 'reference geometry mismatch')
    rng=np.random.Generator(np.random.PCG64(0))
    q=np.linspace(0.0,1.0,1025,dtype=np.float64)
    knots=[]
    for ch in range(3):
        a=baseline.pixels[...,ch].ravel(); b=neutral.pixels[...,ch].ravel()
        a=np.sort(a[rng.integers(0,a.size,min(2_000_000,a.size))])
        b=np.sort(b[rng.integers(0,b.size,min(2_000_000,b.size))])
        knots.append((tuple(float(x) for x in np.quantile(a,q,method='linear')),
                      tuple(float(x) for x in np.quantile(b,q,method='linear'))))
    return ToneMap(authority,tuple(knots))


def apply_saved_map(image, tone_map, *, expected_sha256, context_sha256):
    _versions()
    _digest(expected_sha256); _digest(context_sha256)
    _require(type(image) is Image and image.units==ENCODED and type(tone_map) is ToneMap, 'encoded image and saved map required')
    _require(tone_map.digest==expected_sha256 and tone_map.authority.context_sha256==context_sha256,
             'same frozen map/context required; candidate refitting forbidden')
    result=np.stack([np.interp(image.pixels[...,c],*tone_map.knots[c]) for c in range(3)],axis=2)
    return _image(result,ENCODED)


def _crop(image, x, y):
    h,w=image.pixels.shape[:2]
    _require(0<=x and 0<=y and x+CROP_SIZE<=w and y+CROP_SIZE<=h, 'fixed crop outside image; no exclusion')
    return _image(image.pixels[y:y+CROP_SIZE,x:x+CROP_SIZE],image.units)


def score_false_colour_frame(baseline, neutral, candidate, context, candidate_context, plan, tone_map, *, expected_map_sha256):
    _require(type(context) is Context and context==candidate_context, 'candidate geometry/processing context mismatch')
    ctx=mapping_context(context,plan)
    _require(type(tone_map) is ToneMap and tone_map.authority.context_sha256==ctx, 'map geometry/context mismatch')
    for image in (baseline,neutral,candidate):
        _require(type(image) is Image and image.units==ENCODED, 'encoded input required')
    _require(baseline.digest==tone_map.authority.baseline_sha256 and neutral.digest==tone_map.authority.neutral_sha256,
             'reference inputs changed')
    shape=(context.geometry.height,context.geometry.width,3)
    _require(baseline.pixels.shape==candidate.pixels.shape==shape and
             neutral.pixels.shape==(plan.neutral_height,plan.neutral_width,3), 'frame geometry mismatch')
    mapped=[apply_saved_map(v,tone_map,expected_sha256=expected_map_sha256,context_sha256=ctx) for v in (baseline,candidate)]
    scores=[[],[],[],[],[]]
    for (x,y),(dy,dx) in zip(dict(CROPS)[context.frame],plan.offsets_yx):
        crops=(_crop(mapped[0],x,y),_crop(neutral,x-dx,y-dy),_crop(mapped[1],x,y),
               _crop(baseline,x,y),_crop(candidate,x,y))
        for row,crop in zip(scores,crops):
            row.append(colour_metrics(crop))
    mapped_gate=frame_gap_gates(context.frame,*scores[:3])
    unmapped_gate=frame_gap_gates(context.frame,scores[3],scores[1],scores[4])
    return {'mapped':mapped_gate,'unmapped':unmapped_gate,'metric_rows':scores,
            'map_sha256':tone_map.digest,'context_sha256':ctx,'gap_passed':mapped_gate['passed'],
            'legitimate_colour_controls_required_separately':True}


def legitimate_colour_controls(reference, candidate, *, flat_mask, contrast_pairs, luma_preserving):
    """Linear controls; contrast is Cb/Cr vector distance at fixed endpoint pairs.

    Hue uses atan2(Cr,Cb), shortest angle; reference chroma > .05 is eligible.
    Flat mask/edge endpoints come from a pre-frozen synthetic control design.
    """
    _require(type(reference) is Image and type(candidate) is Image and reference.units==candidate.units==LINEAR,
             'linear sRGB controls required')
    _require(reference.pixels.shape==candidate.pixels.shape,'control shape mismatch')
    h,w=reference.pixels.shape[:2]
    _require(type(flat_mask) is np.ndarray and flat_mask.dtype==np.bool_ and flat_mask.shape==(h,w) and flat_mask.any(),
             'nonempty explicit flat control mask required')
    _require(type(contrast_pairs) is tuple and len(contrast_pairs)>0 and type(luma_preserving) is bool,
             'explicit contrast pairs/luma claim required')
    ry,rcb,rcr=_ycc(reference.pixels);cy,ccb,ccr=_ycc(candidate.pixels)
    for plane in (ry,rcb,rcr,cy,ccb,ccr):
        _require(bool(np.isfinite(plane).all()), 'control transform overflow')
    rch=np.stack((rcb,rcr),axis=2);cch=np.stack((ccb,ccr),axis=2)
    flat_error=float(np.max(np.abs(reference.pixels[flat_mask]-candidate.pixels[flat_mask])))
    luma_error=float(np.max(np.abs(ry-cy)))
    _finite(flat_error); _finite(luma_error)
    contrast=[]
    for pair in contrast_pairs:
        _require(type(pair) is tuple and len(pair)==2, 'two endpoints required')
        for pt in pair:
            _require(type(pt) is tuple and len(pt)==2 and all(type(v) is int for v in pt) and
                     0<=pt[0]<h and 0<=pt[1]<w, 'contrast endpoint outside controls')
        a,b=pair
        r=float(np.linalg.norm(rch[a]-rch[b]));c=float(np.linalg.norm(cch[a]-cch[b]))
        _finite(r); _finite(c)
        contrast.append({'reference':r,'candidate':c,'passed':c>=0.97*r})
    rmag=np.hypot(rcb,rcr);cmag=np.hypot(ccb,ccr);eligible=rmag>0.05
    angles=np.abs(np.arctan2(np.sin(np.arctan2(ccr,ccb)-np.arctan2(rcr,rcb)),
                            np.cos(np.arctan2(ccr,ccb)-np.arctan2(rcr,rcb))))*180.0/np.pi
    hue_max=float(np.max(angles[eligible])) if eligible.any() else None
    hue_ok=not eligible.any() or (bool((cmag[eligible]>0).all()) and hue_max<=1.0)
    passed=flat_error<=1e-6 and all(x['passed'] for x in contrast) and hue_ok and (not luma_preserving or luma_error<=1e-5)
    return {'passed':bool(passed),'flat_max_abs':flat_error,'contrast':contrast,'hue_eligible_pixels':int(eligible.sum()),
            'hue_max_degrees':hue_max,'hue_passed':bool(hue_ok),'luma_max_abs':luma_error,'luma_claim_applied':luma_preserving}
