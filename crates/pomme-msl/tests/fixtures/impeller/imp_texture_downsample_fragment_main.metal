#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float edge;
    float ratio;
    float2 pixel_size;
};

struct imp_texture_downsample_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_texture_downsample_fragment_main_in
{
    float2 v_texture_coords [[user(locn0)]];
};

fragment imp_texture_downsample_fragment_main_out imp_texture_downsample_fragment_main(imp_texture_downsample_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], texture2d<float> texture_sampler [[texture(0)]], sampler texture_samplerSmplr [[sampler(0)]])
{
    imp_texture_downsample_fragment_main_out out = {};
    float _32 = -frag_info.edge;
    float _105;
    float4 _106;
    _106 = float4(0.0);
    _105 = _32;
    float4 _109;
    for (; _105 <= frag_info.edge; _106 = _109, _105 += 2.0)
    {
        _109 = _106;
        for (float _107 = _32; _107 <= frag_info.edge; )
        {
            _109 += (texture_sampler.sample(texture_samplerSmplr, (in.v_texture_coords + (frag_info.pixel_size * float2(_105, _107))), bias(-0.4749999940395355224609375)) * frag_info.ratio);
            _107 += 2.0;
            continue;
        }
    }
    out.frag_color = _106;
    return out;
}

