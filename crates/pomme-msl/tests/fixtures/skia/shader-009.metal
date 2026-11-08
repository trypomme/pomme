#include <metal_stdlib>
#include <simd/simd.h>
#ifdef __clang__
#pragma clang diagnostic ignored "-Wall"
#endif
using namespace metal;
struct sampler2D {texture2d<half> tex;sampler smp;};half4 sample(sampler2D i, float2 p, float b=-0.475) { return i.tex.sample(i.smp, p, bias(b)); }half4 sample(sampler2D i, float3 p, float b=-0.475) { return i.tex.sample(i.smp, p.xy / p.z, bias(b)); }half4 sampleLod(sampler2D i, float2 p, float lod) { return i.tex.sample(i.smp, p, level(lod)); }half4 sampleLod(sampler2D i, float3 p, float lod) {return i.tex.sample(i.smp, p.xy / p.z, level(lod));}half4 sampleGrad(sampler2D i, float2 p, float2 dPdx, float2 dPdy) {return i.tex.sample(i.smp, p, gradient2d(dPdx, dPdy));}struct Inputs {
    float2 vatlasCoord_S0 [[user(locn0)]];
    half4 vcolor_S0 [[user(locn1) flat]];
};
struct Outputs {
    half4 sk_FragColor [[color(0)]];
};
struct uniformBuffer {
float4 sk_RTAdjust;
float2 uatlas_adjust_S0;
};
struct Globals {
    sampler2D uTextureSampler_0_S0;
    constant uniformBuffer* _anonInterface0;
};
fragment Outputs fragmentMain(Inputs _in [[stage_in]], texture2d<half> uTextureSampler_0_S0_Tex [[texture(0)]], sampler uTextureSampler_0_S0_Smplr [[sampler(0)]], constant uniformBuffer& _anonInterface0 [[buffer(0)]], bool _frontFacing [[front_facing]], float4 _fragCoord [[position]]) {
Globals _globals{{uTextureSampler_0_S0_Tex, uTextureSampler_0_S0_Smplr}, &_anonInterface0};
(void)_globals;
Outputs _out;
(void)_out;
half4 outputCoverage_S0 = half4(1.0h);
half atlasCoverage = sample(_globals.uTextureSampler_0_S0, _in.vatlasCoord_S0).x;
outputCoverage_S0 *= atlasCoverage;
half4 outputColor_S0 = _in.vcolor_S0;
{
_out.sk_FragColor = outputColor_S0 * outputCoverage_S0;
}
return _out;
}
