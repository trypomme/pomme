#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float2 center;
    float radius;
    float tile_mode;
    float4 decal_border_color;
    float alpha;
    float2 half_texel;
    float2 focus;
    float focus_radius;
};

struct imp_conical_gradient_fill_strip_radial_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

fragment imp_conical_gradient_fill_strip_radial_fragment_main_out imp_conical_gradient_fill_strip_radial_fragment_main()
{
    imp_conical_gradient_fill_strip_radial_fragment_main_out out = {};
    out.frag_color = float4(0.0);
    return out;
}

