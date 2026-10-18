"""Fixed synthetic known answers. Imports the actual production oracle."""
import dataclasses
import hashlib
import json
import math
import unittest

import numpy as np
import oracle as o

H = 'a'*64
IDENTITY = ((1.0,0.0,0.0),(0.0,1.0,0.0),(0.0,0.0,1.0))


def image(a, units=o.CAMERA):
    return o.Image(np.asarray(a,dtype=np.float64),dtype='float64',units=units)


def context(ref, *, matrix=IDENTITY, wb=(1.0,1.0,1.0), exposure=1.0, seams=(), frame='DSCF2095'):
    h,w=ref.pixels.shape[:2]
    g=o.Geometry(h,w,h,w,0,0,0,H,seams,())
    t=o.Transform(matrix,wb,exposure,ref.digest,H)
    return o.Context(frame,H,1,g,t)


def metric(h,s=100.0):
    return {'hf_chroma_rms':float(h),'luma_gradient':float(s)}


def map_fixture():
    baseline=image(np.full((2,2,3),.2),o.ENCODED)
    neutral=image(np.full((2,2,3),.4),o.ENCODED)
    ctx=context(baseline)
    plan=o.CropPlan(2,2,0,((0,0),)*5,H)
    authority=o.MapAuthority(baseline.digest,neutral.digest,o.mapping_context(ctx,plan),H)
    tone=o.build_reference_map(baseline,neutral,ctx,plan,authority)
    return baseline,neutral,ctx,plan,authority,tone


class OracleTests(unittest.TestCase):
    def test_input_contract(self):
        for a in [np.empty((0,2,3)),np.zeros((2,3)),np.zeros((2,2,4)),
                  np.full((1,1,3),np.nan),np.full((1,1,3),np.inf),np.zeros((1,1,3),dtype=np.uint16)]:
            with self.subTest(shape=a.shape,dtype=str(a.dtype)),self.assertRaises(o.OracleError):
                o.Image(a,dtype='float64',units=o.CAMERA)
        with self.assertRaises(o.OracleError):o.Image(np.zeros((1,1,3),dtype=np.float32),dtype='float64',units=o.CAMERA)
        with self.assertRaises(o.OracleError):o.Image(np.zeros((1,1,3)),dtype='float64',units='unspecified')
        with self.assertRaises(o.OracleError):image([[[-.1,0,0]]],o.ENCODED)
        for dtype in ['float32','float64']:
            self.assertEqual(o.Image(np.zeros((1,1,3),dtype=dtype),dtype=dtype,units=o.LINEAR).dtype,dtype)

    def test_immutable_images_context(self):
        a=np.zeros((1,2,3)); frozen=image(a); digest=frozen.digest;a[:]=1
        self.assertEqual(frozen.digest,digest)
        with self.assertRaises(ValueError):frozen.pixels.setflags(write=True)
        v=frozen.pixels;v.shape=(2,1,3)
        self.assertEqual(frozen.pixels.shape,(1,2,3))
        with self.assertRaises(dataclasses.FrozenInstanceError):context(frozen).passes=3
        with self.assertRaises(o.OracleError):o.Transform([list(x) for x in IDENTITY],(1.,1.,1.),1.,H,H)
        with self.assertRaises(o.OracleError):o.Geometry(2,2,2,2,0,0,0,H,[],())

    def test_wb_inverse_and_validation(self):
        a=image([[[.125,.25,.5]]]);b=o.interpolation_balance(a,4.,(2.,1.,4.,float('nan')))
        np.testing.assert_array_equal(b.pixels,[[[1.,1.,8.]]])
        np.testing.assert_array_equal(o.interpolation_balance(b,4.,(2.,1.,4.),undo=True).pixels,a.pixels)
        self.assertEqual(o.effective_wb((float('nan'),0.,float('inf'))),(1.,1.,1.))
        for wb in [(1.,float('nan'),1.),(1.,1.,0.),(-1.,1.,1.),(1.,1.,float('inf'))]:
            with self.assertRaises(o.OracleError):o.interpolation_balance(a,1.,wb)
        for scale in [0.,-1.,float('inf')]:
            with self.assertRaises(o.OracleError):o.interpolation_balance(a,scale,(1.,1.,1.))

    def test_fixed_matrix_and_context(self):
        ref=image([[[.125,.25,.5]]]);ctx=context(ref,matrix=((2.,0.,0.),(0.,3.,0.),(0.,0.,4.)),wb=(2.,1.,4.),exposure=.5)
        np.testing.assert_array_equal(o.linear_srgb(ref,ctx.transform).pixels,[[[.25,.375,4.]]])
        np.testing.assert_array_equal(np.linalg.inv(np.array(ctx.transform.camera_to_srgb)),np.diag([.5,1/3,.25]))
        with self.assertRaises(o.OracleError):o.Transform(((1.,1.,1.),)*3,(1.,1.,1.),1.,H,H)
        for changed in [dataclasses.replace(ctx,passes=3),dataclasses.replace(ctx,source_sha256='b'*64),
                        dataclasses.replace(ctx,transform=dataclasses.replace(ctx.transform,exposure=2.))]:
            with self.assertRaises(o.OracleError):o.compare_cpu(ref,ref,ctx,changed)
        wrong=dataclasses.replace(ctx,transform=dataclasses.replace(ctx.transform,reference_sha256='c'*64))
        with self.assertRaises(o.OracleError):o.compare_cpu(ref,ref,wrong,wrong)

    def test_transfer_and_rounding(self):
        enc,clipped=o.encode_srgb(image([[[0.,.0031308,1.]]],o.LINEAR))
        np.testing.assert_allclose(enc.pixels,[[[0.,.040449936,1.]]],rtol=0,atol=2e-16)
        self.assertEqual(clipped,{'below_zero':0,'above_one':0})
        ties=image([[[.5/255,1.5/255,254.5/255]]],o.ENCODED)
        np.testing.assert_array_equal(o.quantize_srgb(ties),[[[1.,2.,255.]]])

    def test_error_rank_all_channels(self):
        ref=image(np.zeros((10,10,3)));a=np.zeros((10,10,3));a.ravel()[-4:]=[1,2,3,100]
        r=o.compare_cpu(image(a),ref,context(ref),context(ref))['errors']['linear_camera']['whole']
        self.assertEqual(r['channels'],300);self.assertEqual(r['mean'],106/300)
        self.assertEqual(r['p99'],1.);self.assertEqual(r['max'],100.)
        ref=image([[[0.,0.,0.]]]);r=o.compare_cpu(image([[[0.,1.,2.]]]),ref,context(ref),context(ref))
        self.assertEqual(r['errors']['linear_camera']['whole']['p99'],2.)

    def test_signed_hdr_clipping_and_cpu_gate(self):
        ref=image([[[-1.,2.,.5]]]);candidate=image([[[-2.,4.,.5]]]);ctx=context(ref)
        cpu=o.compare_cpu(candidate,ref,ctx,ctx)
        self.assertTrue(cpu['passed']);self.assertEqual(cpu['errors']['linear_camera']['whole']['mean'],1.)
        self.assertEqual(cpu['errors']['linear_camera']['whole']['max'],2.)
        self.assertEqual(cpu['clipped'],{'candidate':{'below_zero':1,'above_one':1},'reference':{'below_zero':1,'above_one':1}})
        self.assertFalse(o.compare_gpu(candidate,ref,ctx,ctx)['passed'])
        white=image([[[1.,1.,1.]]]);black=image([[[0.,0.,0.]]]);ctx=context(black)
        self.assertFalse(o.compare_cpu(white,black,ctx,ctx)['passed'])

    def test_whole_border_seam_inclusion(self):
        ref=image(np.zeros((40,40,3)));a=np.zeros((40,40,3));a[0,0]=[1,1,1];ctx=context(ref,seams=(20,))
        r=o.compare_cpu(image(a),ref,ctx,ctx)
        self.assertEqual(r['excluded_channels'],0)
        stats=r['errors']['linear_camera'];self.assertEqual(stats['whole']['channels'],4800)
        self.assertEqual(stats['whole']['mean'],3/4800)
        self.assertEqual(stats['regions']['border8']['channels'],3072)
        self.assertEqual(stats['regions']['seam16']['channels'],1920)
        self.assertEqual(stats['regions']['interior']['channels'],576)
        self.assertEqual(stats['regions']['border8']['stats']['max'],1.)
        a=np.zeros((40,40,3));a[20,20,1]=2.
        r=o.compare_cpu(image(a),ref,ctx,ctx)['errors']['linear_camera']
        self.assertEqual(r['whole']['max'],2.);self.assertEqual(r['regions']['seam16']['stats']['max'],2.)
        small=image([[[0.,0.,0.]]]);r=o.compare_cpu(small,small,context(small),context(small))
        self.assertIsNone(r['errors']['linear_camera']['regions']['interior']['stats'])

    def test_gpu_thresholds_three_spaces(self):
        ref=image(np.full((10,10,3),.25));ctx=context(ref)
        self.assertTrue(o.compare_gpu(image(np.nextafter(ref.pixels+1e-5,ref.pixels)),ref,ctx,ctx)['passed'])
        self.assertFalse(o.compare_gpu(image(ref.pixels+1.01e-5),ref,ctx,ctx)['passed'])
        a=ref.pixels.copy();a[5,5,1]+=2e-3
        self.assertFalse(o.compare_gpu(image(a),ref,ctx,ctx)['passed'])
        dark=image(np.zeros((10,10,3)));ctx=context(dark,matrix=((2.,0.,0.),(0.,2.,0.),(0.,0.,2.)))
        r=o.compare_gpu(image(dark.pixels+8e-6),dark,ctx,ctx)
        self.assertTrue(r['space_passed']['linear_camera']);self.assertFalse(r['space_passed']['linear_srgb'])
        ctx=context(dark);r=o.compare_gpu(image(dark.pixels+8e-6),dark,ctx,ctx)
        self.assertTrue(r['space_passed']['linear_srgb']);self.assertFalse(r['space_passed']['display_float'])

    def test_scipy_reflect_known_answer(self):
        r=o.colour_metrics(image([[[0.,0.,0.],[1.,0.,0.]]],o.ENCODED))
        self.assertAlmostEqual(r['hf_chroma_rms'],153*math.sqrt(.2126**2+.7874**2),places=11)
        self.assertAlmostEqual(r['luma_gradient'],216.852,places=11)
        r=o.colour_metrics(image(np.full((9,9,3),.4),o.ENCODED))
        self.assertAlmostEqual(r['hf_chroma_rms'],0.,places=12);self.assertEqual(r['luma_gradient'],0.)

    def test_frozen_crop_coordinates_and_bounds(self):
        self.assertEqual(o.CROP_SIZE,140)
        self.assertEqual(o.CROPS,(('DSCF2095',((3180,2088),(3660,1615),(3518,393),(2200,2400),(2888,1425))),
                                  ('DSCF1895',((2450,318),(2165,1142),(2219,1923),(2279,1473),(2501,786)))))
        a=image(np.zeros((140,140,3)),o.ENCODED)
        self.assertEqual(o._crop(a,0,0).pixels.shape,(140,140,3))
        with self.assertRaises(o.OracleError):o._crop(a,1,0)

    def test_gap_positive_nonpositive(self):
        r=o.gap_gate(metric(10),metric(2),metric(6,97));self.assertTrue(r['passed']);self.assertEqual(r['chroma_limit'],6.)
        self.assertFalse(o.gap_gate(metric(10),metric(2),metric(6.01,97))['passed'])
        self.assertFalse(o.gap_gate(metric(10),metric(2),metric(6,96.9))['passed'])
        self.assertTrue(o.gap_gate(metric(2),metric(4),metric(2))['passed'])
        self.assertFalse(o.gap_gate(metric(2),metric(4),metric(2.01))['passed'])
        self.assertFalse(o.gap_gate(metric(2),metric(2),metric(2.01))['passed'])
        for bad in [-1.,float('nan'),float('inf')]:
            with self.assertRaises(o.OracleError):o.gap_gate(metric(bad),metric(1),metric(1))

    def test_crop_failure_not_hidden_by_means(self):
        r=o.frame_gap_gates('DSCF2095',[metric(10)]*5,[metric(2)]*5,[metric(7)]+[metric(0)]*4)
        self.assertTrue(r['frame_means']['passed']);self.assertFalse(r['crops'][0]['passed']);self.assertFalse(r['passed'])
        r=o.frame_gap_gates('DSCF2095',[metric(10)]*5,[metric(2)]*5,[metric(0,96)]+[metric(0)]*4)
        self.assertFalse(r['passed'])
        with self.assertRaises(o.OracleError):o.frame_gap_gates('DSCF2095',[metric(10)]*4,[metric(2)]*4,[metric(0)]*4)

    def test_both_frames_required(self):
        good=o.frame_gap_gates('DSCF2095',[metric(10)]*5,[metric(2)]*5,[metric(6)]*5)
        good2=o.frame_gap_gates('DSCF1895',[metric(10)]*5,[metric(2)]*5,[metric(6)]*5)
        bad=o.frame_gap_gates('DSCF1895',[metric(10)]*5,[metric(2)]*5,[metric(7)]*5)
        self.assertTrue(o.both_frames_gate([good,good2]));self.assertFalse(o.both_frames_gate([good,bad]))
        for values in [[good],[good,good]]:
            with self.assertRaises(o.OracleError):o.both_frames_gate(values)

    def test_reference_map_deterministic_reuse(self):
        b,n,c,p,a,t=map_fixture();t2=o.build_reference_map(b,n,c,p,a)
        self.assertEqual(t.to_json(),t2.to_json())
        for x,y in t.knots:
            self.assertEqual(x,(.2,)*1025);self.assertEqual(y,(.4,)*1025)
        result=o.apply_saved_map(image(np.full((2,2,3),.8),o.ENCODED),t,expected_sha256=t.digest,context_sha256=a.context_sha256)
        np.testing.assert_array_equal(result.pixels,np.full((2,2,3),.4))
        self.assertEqual(t.digest,t2.digest)

    def test_map_candidate_refit_context_rejected(self):
        b,n,c,p,a,t=map_fixture();candidate=image(np.full((2,2,3),.8),o.ENCODED)
        with self.assertRaises(o.OracleError):o.build_reference_map(candidate,n,c,p,a)
        with self.assertRaises(o.OracleError):o.build_reference_map(b,n,dataclasses.replace(c,passes=3),p,a)
        with self.assertRaises(o.OracleError):o.build_reference_map(b,n,c,dataclasses.replace(p,offsets_yx=((1,0),)*5),a)
        with self.assertRaises(o.OracleError):o.apply_saved_map(candidate,t,expected_sha256='b'*64,context_sha256=a.context_sha256)
        with self.assertRaises(o.OracleError):o.apply_saved_map(candidate,t,expected_sha256=t.digest,context_sha256='c'*64)

    def test_map_serialization_integrity(self):
        *_,t=map_fixture();s=t.to_json()
        self.assertEqual(o.ToneMap.from_json(s,expected_sha256=t.digest).to_json(),s)
        mutations=[]
        d=json.loads(s);d['extra']=0;mutations.append(json.dumps(d))
        mutations.extend([s.replace('"seed":0','"seed":0,"seed":0'),s.replace('"seed":0','"seed":0,"s\\u0065ed":0')])
        d=json.loads(s);d['seed']=1;mutations.append(json.dumps(d))
        d=json.loads(s);d['knots'][0][0][0]=.9;mutations.append(json.dumps(d))
        mutations.extend(['{',' '*1_048_577])
        for text in mutations:
            with self.subTest(prefix=text[:40]),self.assertRaises(o.OracleError):
                o.ToneMap.from_json(text,expected_sha256=hashlib.sha256(text.encode()).hexdigest())

    def test_control_flat_neutral_saturated(self):
        ref=image([[[.2,.2,.2],[1.,0.,0.],[0.,0.,1.]]],o.LINEAR);mask=np.ones((1,3),bool);pairs=(((0,1),(0,2)),)
        self.assertTrue(o.legitimate_colour_controls(ref,ref,flat_mask=mask,contrast_pairs=pairs,luma_preserving=True)['passed'])
        a=ref.pixels.copy();a[0,0,0]+=2e-6
        self.assertFalse(o.legitimate_colour_controls(ref,image(a,o.LINEAR),flat_mask=mask,contrast_pairs=pairs,luma_preserving=False)['passed'])

    def test_control_contrast_hue_luma(self):
        ref=image([[[.2,.2,.2],[1.,0.,0.],[0.,0.,1.]]],o.LINEAR);mask=np.array([[True,False,False]]);pairs=(((0,1),(0,2)),)
        for scale,want in [(.98,True),(.96,False)]:
            a=ref.pixels.copy();a[:,1:]*=scale
            r=o.legitimate_colour_controls(ref,image(a,o.LINEAR),flat_mask=mask,contrast_pairs=pairs,luma_preserving=False)
            self.assertEqual(r['passed'],want)
        a=ref.pixels.copy();a[0,1]=[0.,0.,1.]
        self.assertFalse(o.legitimate_colour_controls(ref,image(a,o.LINEAR),flat_mask=mask,contrast_pairs=pairs,luma_preserving=False)['hue_passed'])
        a=ref.pixels.copy();a[0,1]+=2e-5
        r=o.legitimate_colour_controls(ref,image(a,o.LINEAR),flat_mask=mask,contrast_pairs=pairs,luma_preserving=True)
        self.assertFalse(r['passed']);self.assertGreater(r['luma_max_abs'],1e-5)

    def test_false_colour_binding_and_no_crop_skip(self):
        b,n,c,p,a,t=map_fixture()
        with self.assertRaisesRegex(o.OracleError,'outside image'):
            o.score_false_colour_frame(b,n,b,c,c,p,t,expected_map_sha256=t.digest)
        with self.assertRaises(o.OracleError):
            o.score_false_colour_frame(b,n,b,c,dataclasses.replace(c,passes=3),p,t,expected_map_sha256=t.digest)
        with self.assertRaises(o.OracleError):
            o.score_false_colour_frame(b,n,b,c,c,p,t,expected_map_sha256='b'*64)
        changed=image(np.full((2,2,3),.3),o.ENCODED)
        with self.assertRaises(o.OracleError):
            o.score_false_colour_frame(changed,n,b,c,c,p,t,expected_map_sha256=t.digest)


if __name__ == '__main__':
    unittest.main()
