#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float4 shadow_color;
};

struct imp_shadow_vertices_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_shadow_vertices_fragment_main_in
{
    float v_gaussian [[user(locn0)]];
};

fragment imp_shadow_vertices_fragment_main_out imp_shadow_vertices_fragment_main(imp_shadow_vertices_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]])
{
    imp_shadow_vertices_fragment_main_out out = {};
    float _96 = (in.v_gaussian * 4.0) - 2.0;
    float _106 = abs(_96);
    float _117 = ((0.2783930003643035888671875 + ((0.23038899898529052734375 + ((0.07810799777507781982421875 * _106) * _106)) * _106)) * _106) + 1.0;
    out.frag_color = frag_info.shadow_color * ((1.0 + (sign(_96) * (1.0 - (1.0 / (((_117 * _117) * _117) * _117))))) * 0.5);
    return out;
}

