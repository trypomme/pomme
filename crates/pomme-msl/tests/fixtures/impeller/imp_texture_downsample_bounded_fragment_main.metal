#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float edge;
    float ratio;
    float2 pixel_size;
};

struct BoundInfo
{
    float4x4 quad_line_params;
};

struct imp_texture_downsample_bounded_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_texture_downsample_bounded_fragment_main_in
{
    float2 v_texture_coords [[user(locn0)]];
};

fragment imp_texture_downsample_bounded_fragment_main_out imp_texture_downsample_bounded_fragment_main(imp_texture_downsample_bounded_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], constant BoundInfo& bound_info [[buffer(1)]], texture2d<float> texture_sampler [[texture(0)]], sampler texture_samplerSmplr [[sampler(0)]])
{
    imp_texture_downsample_bounded_fragment_main_out out = {};
    float _37 = -frag_info.edge;
    float4 _169;
    _169 = float4(0.0);
    float4 _174;
    for (float _168 = _37; _168 <= frag_info.edge; _169 = _174, _168 += 2.0)
    {
        _174 = _169;
        float4 _79;
        for (float _170 = _37; _170 <= frag_info.edge; _174 = _79, _170 += 2.0)
        {
            float2 _71 = in.v_texture_coords + (frag_info.pixel_size * float2(_168, _170));
            float4 _172;
            do
            {
                if (any((float4(_71, 1.0, 0.0) * bound_info.quad_line_params) < float4(0.0)))
                {
                    _172 = float4(0.0);
                    break;
                }
                _172 = texture_sampler.sample(texture_samplerSmplr, _71, bias(-0.4749999940395355224609375));
                break;
            } while(false);
            _79 = _174 + (_172 * frag_info.ratio);
        }
    }
    out.frag_color = _169;
    return out;
}

