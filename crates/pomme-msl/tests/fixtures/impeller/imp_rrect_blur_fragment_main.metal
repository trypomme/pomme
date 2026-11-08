#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float4 color;
    float4 center_adjust;
    float3 r1_exponent_exponentInv;
    float3 sInv_minEdge_scale;
};

struct imp_rrect_blur_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_rrect_blur_fragment_main_in
{
    float2 v_position [[user(locn0)]];
};

fragment imp_rrect_blur_fragment_main_out imp_rrect_blur_fragment_main(imp_rrect_blur_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]])
{
    imp_rrect_blur_fragment_main_out out = {};
    float2 _240 = abs(in.v_position - frag_info.center_adjust.xy) - frag_info.center_adjust.zw;
    float2 _243 = fast::max(_240, float2(0.0));
    float _254 = (powr(powr(_243.x, frag_info.r1_exponent_exponentInv.y) + powr(_243.y, frag_info.r1_exponent_exponentInv.y), frag_info.r1_exponent_exponentInv.z) + fast::min(fast::max(_240.x, _240.y), 0.0)) - frag_info.r1_exponent_exponentInv.x;
    float _309 = (frag_info.sInv_minEdge_scale.x * (frag_info.sInv_minEdge_scale.y + _254)) * 1.12837922573089599609375;
    float _312 = _309 * _309;
    float _324 = _309 + ((0.24295000731945037841796875 + ((0.0339500010013580322265625 + (0.010400000028312206268310546875 * _312)) * _312)) * (_309 * _312));
    float _336 = (frag_info.sInv_minEdge_scale.x * _254) * 1.12837922573089599609375;
    float _339 = _336 * _336;
    float _351 = _336 + ((0.24295000731945037841796875 + ((0.0339500010013580322265625 + (0.010400000028312206268310546875 * _339)) * _339)) * (_336 * _339));
    out.frag_color = frag_info.color * (frag_info.sInv_minEdge_scale.z * ((_324 / sqrt(1.0 + (_324 * _324))) - (_351 / sqrt(1.0 + (_351 * _351)))));
    return out;
}

