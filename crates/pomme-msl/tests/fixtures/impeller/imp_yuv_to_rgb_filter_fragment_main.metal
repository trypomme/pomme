#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float4x4 matrix;
    float yuv_color_space;
};

struct imp_yuv_to_rgb_filter_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_yuv_to_rgb_filter_fragment_main_in
{
    float2 v_texture_coords [[user(locn0)]];
};

fragment imp_yuv_to_rgb_filter_fragment_main_out imp_yuv_to_rgb_filter_fragment_main(imp_yuv_to_rgb_filter_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], texture2d<float> y_texture [[texture(0)]], texture2d<float> uv_texture [[texture(1)]], sampler y_textureSmplr [[sampler(0)]], sampler uv_textureSmplr [[sampler(1)]])
{
    imp_yuv_to_rgb_filter_fragment_main_out out = {};
    out.frag_color = frag_info.matrix * float4(float3(y_texture.sample(y_textureSmplr, in.v_texture_coords).x, uv_texture.sample(uv_textureSmplr, in.v_texture_coords).xy) - select(float3(0.0, 0.5, 0.5), float3(0.062745101749897003173828125, 0.5, 0.5), bool3(frag_info.yuv_color_space == 0.0)), 1.0);
    return out;
}

