#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float4 color;
    float4 center_adjust;
    float3 r1_exponent_exponentInv;
    float3 sInv_minEdge_scale;
    float4 halfAxes_retractionDepth;
    float4 infoTop;
    float4 infoRight;
    float4 polyTop;
    float4 polyRight;
};

struct imp_rsuperellipse_blur_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_rsuperellipse_blur_fragment_main_in
{
    float2 v_position [[user(locn0)]];
};

fragment imp_rsuperellipse_blur_fragment_main_out imp_rsuperellipse_blur_fragment_main(imp_rsuperellipse_blur_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]])
{
    imp_rsuperellipse_blur_fragment_main_out out = {};
    float2 _186 = abs(in.v_position - frag_info.center_adjust.xy);
    float2 _393 = _186 - frag_info.center_adjust.zw;
    float2 _396 = fast::max(_393, float2(0.0));
    float _407 = (powr(powr(_396.x, frag_info.r1_exponent_exponentInv.y) + powr(_396.y, frag_info.r1_exponent_exponentInv.y), frag_info.r1_exponent_exponentInv.z) + fast::min(fast::max(_393.x, _393.y), 0.0)) - frag_info.r1_exponent_exponentInv.x;
    float _212 = frag_info.halfAxes_retractionDepth.y - frag_info.halfAxes_retractionDepth.x;
    float _217 = _186.y;
    float _219 = _217 - _212;
    float _221 = _186.x;
    bool _222 = _219 > _221;
    float4 _536;
    if (_222)
    {
        _536 = frag_info.infoTop;
    }
    else
    {
        _536 = frag_info.infoRight;
    }
    float _537;
    if (_222)
    {
        _537 = _221 / _219;
    }
    else
    {
        _537 = _217 / (_221 + _212);
    }
    float _258 = atan(_537);
    float _545;
    if (_258 < _536.x)
    {
        _545 = (1.0 - powr(1.0 + powr(tan(_258), _536.z), _536.w)) * (_222 ? frag_info.halfAxes_retractionDepth.x : frag_info.halfAxes_retractionDepth.y);
    }
    else
    {
        float _307 = (_258 - _536.x) / (0.78539812564849853515625 - _536.x);
        float _311 = _307 * _307;
        float4 _538;
        if (_222)
        {
            _538 = frag_info.polyTop;
        }
        else
        {
            _538 = frag_info.polyRight;
        }
        float _333 = dot(float4(_311 * _307, _311, _307, 1.0), _538);
        _545 = (_333 * _333) * _536.y;
    }
    float _350 = _407 + (_545 * smoothstep(-frag_info.halfAxes_retractionDepth.z, 0.0, -abs(_407)));
    float _462 = (frag_info.sInv_minEdge_scale.x * (frag_info.sInv_minEdge_scale.y + _350)) * 1.12837922573089599609375;
    float _465 = _462 * _462;
    float _477 = _462 + ((0.24295000731945037841796875 + ((0.0339500010013580322265625 + (0.010400000028312206268310546875 * _465)) * _465)) * (_462 * _465));
    float _489 = (frag_info.sInv_minEdge_scale.x * _350) * 1.12837922573089599609375;
    float _492 = _489 * _489;
    float _504 = _489 + ((0.24295000731945037841796875 + ((0.0339500010013580322265625 + (0.010400000028312206268310546875 * _492)) * _492)) * (_489 * _492));
    out.frag_color = frag_info.color * (frag_info.sInv_minEdge_scale.z * ((_477 / sqrt(1.0 + (_477 * _477))) - (_504 / sqrt(1.0 + (_504 * _504)))));
    return out;
}

