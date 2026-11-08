#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float2 center;
    float2 focus;
    float focus_radius;
    float radius;
    float tile_mode;
    float alpha;
    float colors_length;
    float4 decal_border_color;
};

struct StopPairsInfo
{
    float4 stop_pairs[128];
};

struct ColorsInfo
{
    float4 colors[256];
};

struct imp_conical_gradient_uniform_fill_strip_radial_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

fragment imp_conical_gradient_uniform_fill_strip_radial_fragment_main_out imp_conical_gradient_uniform_fill_strip_radial_fragment_main(constant FragInfo& frag_info [[buffer(0)]])
{
    imp_conical_gradient_uniform_fill_strip_radial_fragment_main_out out = {};
    out.frag_color = float4(frag_info.decal_border_color.xyz * frag_info.decal_border_color.w, frag_info.decal_border_color.w) * frag_info.alpha;
    return out;
}

