#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float4 color;
    float2 center;
    float radius;
    float stroke_width;
    float aa_pixels;
    float stroked;
};

struct imp_circle_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_circle_fragment_main_in
{
    float2 v_position [[user(locn0)]];
};

fragment imp_circle_fragment_main_out imp_circle_fragment_main(imp_circle_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]])
{
    imp_circle_fragment_main_out out = {};
    float2 _91 = in.v_position - frag_info.center;
    float _94 = length(_91);
    float2 _256;
    if (_94 > 0.0)
    {
        _256 = _91 / float2(_94);
    }
    else
    {
        _256 = float2(1.0, 0.0);
    }
    float _125 = dot(float2(length(dfdx(in.v_position)), length(dfdy(in.v_position))), abs(_256));
    float _144 = fast::max(frag_info.stroke_width, _125) * 0.5;
    float _164 = _125 * frag_info.aa_pixels;
    out.frag_color = float4(frag_info.color.xyz, frag_info.color.w * (1.0 - smoothstep(_164 * (-0.5), _164 * 0.5, mix(_94 - frag_info.radius, fast::max(-(_94 - (frag_info.radius - _144)), _94 - (frag_info.radius + _144)), frag_info.stroked))));
    out.frag_color = float4(out.frag_color.xyz * out.frag_color.w, out.frag_color.w);
    return out;
}

