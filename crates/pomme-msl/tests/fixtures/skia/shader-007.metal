#include <metal_stdlib>
#include <simd/simd.h>
#ifdef __clang__
#pragma clang diagnostic ignored "-Wall"
#endif
using namespace metal;
struct Inputs {
    float4 vinCircleEdge_S0 [[user(locn0)]];
    half4 vinColor_S0 [[user(locn1)]];
    float2 vTransformedCoords_5_S0 [[user(locn2)]];
};
struct Outputs {
    half4 sk_FragColor [[color(0)]];
};
struct uniformBuffer {
float4 sk_RTAdjust;
float4 ulocalMatrix_S0;
half4 ustart_S1_c0_c0;
half4 uend_S1_c0_c0;
float3x3 umatrix_S1_c0_c1;
half4 uleftBorderColor_S1_c0;
half4 urightBorderColor_S1_c0;
};
struct Globals {
    constant uniformBuffer* _anonInterface0;
};
fragment Outputs fragmentMain(Inputs _in [[stage_in]], constant uniformBuffer& _anonInterface0 [[buffer(0)]], bool _frontFacing [[front_facing]], float4 _fragCoord [[position]]) {
Globals _globals{&_anonInterface0};
(void)_globals;
Outputs _out;
(void)_out;
float4 circleEdge = _in.vinCircleEdge_S0;
half4 outputColor_S0 = _in.vinColor_S0;
float d = length(circleEdge.xy);
half distanceToOuterEdge = half(circleEdge.z * (1.0 - d));
half edgeAlpha = saturate(distanceToOuterEdge);
half4 outputCoverage_S0 = half4(edgeAlpha);
half4 _13_input = outputColor_S0;
float2 _16_tmp_3_coords = _in.vTransformedCoords_5_S0;
half4 _17_t = half4(half(_16_tmp_3_coords.x + 1e-05), 1.0h, 0.0h, 0.0h);
half4 _18_outColor;
if (_17_t.x < 0.0h) {
_18_outColor = _globals._anonInterface0->uleftBorderColor_S1_c0;
} else if (_17_t.x > 1.0h) {
_18_outColor = _globals._anonInterface0->urightBorderColor_S1_c0;
} else {
float2 _20_tmp_1_coords = float2(half2(_17_t.x, 0.0h));
_18_outColor = mix(_globals._anonInterface0->ustart_S1_c0_c0, _globals._anonInterface0->uend_S1_c0_c0, half(_20_tmp_1_coords.x));
}
_13_input = _18_outColor;
half4 output_S1 = _13_input;
{
_out.sk_FragColor = output_S1 * outputCoverage_S0;
}
return _out;
}
