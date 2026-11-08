#include <metal_stdlib>
#include <simd/simd.h>
#ifdef __clang__
#pragma clang diagnostic ignored "-Wall"
#endif
using namespace metal;
constant const float PRECISION = 4.0;
constant const float MAX_FIXED_RESOLVE_LEVEL = 5.0;
constant const float MAX_FIXED_SEGMENTS = 32.0;
struct Inputs {
    float2 resolveLevel_and_idx  [[attribute(0)]];
    float4 p01  [[attribute(1)]];
    float4 p23  [[attribute(2)]];
    float2 fanPointAttrib  [[attribute(3)]];
};
struct Outputs {
    float4 sk_Position [[position]];
    float sk_PointSize [[point_size]];
};
struct uniformBuffer {
float4 sk_RTAdjust;
float4 uaffineMatrix_S0;
float2 utranslate_S0;
half4 ucolor_S0;
};
struct Globals {
    constant uniformBuffer* _anonInterface0;
};
float wangs_formula_max_fdiff_p2_ff2f2f2f2f22(float2 p0, float2 p1, float2 p2, float2 p3, float2x2 matrix) {
float2 d0 = matrix * (fma(float2(-2.0), p1, p2) + p0);
float2 d1 = matrix * (fma(float2(-2.0), p2, p3) + p1);
return max(dot(d0, d0), dot(d1, d1));
}
float wangs_formula_conic_p2_fff2f2f2f(float _precision_, float2 p0, float2 p1, float2 p2, float w) {
float2 C = (min(min(p0, p1), p2) + max(max(p0, p1), p2)) * 0.5;
p0 -= C;
p1 -= C;
p2 -= C;
float m = sqrt(max(max(dot(p0, p0), dot(p1, p1)), dot(p2, p2)));
float2 dp = fma(float2(-2.0 * w), p1, p0) + p2;
float dw = abs(fma(-2.0, w, 2.0));
float rp_minus_1 = max(0.0, fma(m, _precision_, -1.0));
float numer = length(dp) * _precision_ + rp_minus_1 * dw;
float denom = 4.0 * min(w, 1.0);
return numer / denom;
}
vertex Outputs vertexMain(Inputs _in [[stage_in]], constant uniformBuffer& _anonInterface0 [[buffer(0)]], uint sk_VertexID [[vertex_id]], uint sk_InstanceID [[instance_id]]) {
Globals _globals{&_anonInterface0};
(void)_globals;
Outputs _out;
(void)_out;
float2x2 AFFINE_MATRIX = float2x2(_globals._anonInterface0->uaffineMatrix_S0.xy, _globals._anonInterface0->uaffineMatrix_S0.zw);
float2 TRANSLATE = _globals._anonInterface0->utranslate_S0;
float resolveLevel = _in.resolveLevel_and_idx.x;
float idxInResolveLevel = _in.resolveLevel_and_idx.y;
float2 localcoord;
if (resolveLevel < 0.0) {
localcoord = _in.fanPointAttrib;
} else {
if (isinf(_in.p23.z)) {
localcoord = resolveLevel != 0.0 ? _in.p01.zw : (idxInResolveLevel != 0.0 ? _in.p23.xy : _in.p01.xy);
} else {
float2 p0 = _in.p01.xy;
float2 p1 = _in.p01.zw;
float2 p2 = _in.p23.xy;
float2 p3 = _in.p23.zw;
float w = -1.0;
float maxResolveLevel;
if (isinf(_in.p23.w)) {
w = p3.x;
float _0_n2 = wangs_formula_conic_p2_fff2f2f2f(PRECISION, AFFINE_MATRIX * p0, AFFINE_MATRIX * p1, AFFINE_MATRIX * p2, w);
maxResolveLevel = ceil(log2(max(_0_n2, 1.0)) * 0.5);
p1 *= w;
p3 = p2;
} else {
float _1_m = wangs_formula_max_fdiff_p2_ff2f2f2f2f22(p0, p1, p2, p3, AFFINE_MATRIX);
maxResolveLevel = ceil(log2(max(9.0 * _1_m, 1.0)) * 0.25);
}
if (resolveLevel > maxResolveLevel) {
idxInResolveLevel = floor(idxInResolveLevel * exp2(maxResolveLevel - resolveLevel));
resolveLevel = maxResolveLevel;
}
float fixedVertexID = floor(0.5 + idxInResolveLevel * exp2(MAX_FIXED_RESOLVE_LEVEL - resolveLevel));
if (0.0 < fixedVertexID && fixedVertexID < MAX_FIXED_SEGMENTS) {
float T = fixedVertexID * 0.03125;
float2 ab = mix(p0, p1, T);
float2 bc = mix(p1, p2, T);
float2 cd = mix(p2, p3, T);
float2 abc = mix(ab, bc, T);
float2 bcd = mix(bc, cd, T);
float2 abcd = mix(abc, bcd, T);
float u = mix(1.0, w, T);
float v = (w + 1.0) - u;
float uv = mix(u, v, T);
localcoord = w < 0.0 ? abcd : abc / uv;
} else {
localcoord = fixedVertexID == 0.0 ? p0 : p3;
}
}
}
float2 vertexpos = AFFINE_MATRIX * localcoord + TRANSLATE;
_out.sk_Position = float4(vertexpos, 0.0, 1.0);
_out.sk_Position = float4(_out.sk_Position.xy * _globals._anonInterface0->sk_RTAdjust.xz + _out.sk_Position.ww * _globals._anonInterface0->sk_RTAdjust.yw, 0.0, _out.sk_Position.w);
return _out;
}
