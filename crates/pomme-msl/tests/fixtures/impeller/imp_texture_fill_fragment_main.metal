#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float alpha;
};

struct imp_texture_fill_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_texture_fill_fragment_main_in
{
    float2 v_texture_coords [[user(locn0)]];
};

fragment imp_texture_fill_fragment_main_out imp_texture_fill_fragment_main(imp_texture_fill_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], texture2d<float> texture_sampler [[texture(0)]], sampler texture_samplerSmplr [[sampler(0)]])
{
    imp_texture_fill_fragment_main_out out = {};
    out.frag_color = texture_sampler.sample(texture_samplerSmplr, in.v_texture_coords, bias(-0.4749999940395355224609375)) * frag_info.alpha;
    return out;
}

