#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float4 source_rect;
    float alpha;
};

struct imp_texture_fill_strict_src_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_texture_fill_strict_src_fragment_main_in
{
    float2 v_texture_coords [[user(locn0)]];
};

fragment imp_texture_fill_strict_src_fragment_main_out imp_texture_fill_strict_src_fragment_main(imp_texture_fill_strict_src_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], texture2d<float> texture_sampler [[texture(0)]], sampler texture_samplerSmplr [[sampler(0)]])
{
    imp_texture_fill_strict_src_fragment_main_out out = {};
    out.frag_color = texture_sampler.sample(texture_samplerSmplr, float2(fast::clamp(in.v_texture_coords.x, frag_info.source_rect.x, frag_info.source_rect.z), fast::clamp(in.v_texture_coords.y, frag_info.source_rect.y, frag_info.source_rect.w)), bias(-0.4749999940395355224609375)) * frag_info.alpha;
    return out;
}

