#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float4 color;
};

struct imp_solid_fill_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

fragment imp_solid_fill_fragment_main_out imp_solid_fill_fragment_main(constant FragInfo& frag_info [[buffer(0)]])
{
    imp_solid_fill_fragment_main_out out = {};
    out.frag_color = frag_info.color;
    return out;
}

