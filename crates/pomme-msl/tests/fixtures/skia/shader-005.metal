#include <metal_stdlib>
#include <simd/simd.h>
#ifdef __clang__
#pragma clang diagnostic ignored "-Wall"
#endif
using namespace metal;
struct Inputs {
    float4 vinCircleEdge_S0 [[user(locn0)]];
    half4 vinColor_S0 [[user(locn1)]];
};
struct Outputs {
    half4 sk_FragColor [[color(0)]];
};
struct uniformBuffer {
float4 sk_RTAdjust;
float4 ulocalMatrix_S0;
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
{
_out.sk_FragColor = outputColor_S0 * outputCoverage_S0;
}
return _out;
}
