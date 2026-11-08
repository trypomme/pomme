#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float input_alpha;
};

struct imp_linear_to_srgb_filter_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_linear_to_srgb_filter_fragment_main_in
{
    float2 v_texture_coords [[user(locn0)]];
};

fragment imp_linear_to_srgb_filter_fragment_main_out imp_linear_to_srgb_filter_fragment_main(imp_linear_to_srgb_filter_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], texture2d<float> input_texture [[texture(0)]], sampler input_textureSmplr [[sampler(0)]])
{
    imp_linear_to_srgb_filter_fragment_main_out out = {};
    float4 _71 = input_texture.sample(input_textureSmplr, in.v_texture_coords);
    float4 _80 = _71 * frag_info.input_alpha;
    float4 _187;
    do
    {
        float _151 = _80.w;
        if (_151 == 0.0)
        {
            _187 = float4(0.0);
            break;
        }
        _187 = float4(_80.xyz / float3(_151), _151);
        break;
    } while(false);
    float4 color = _187;
    for (int _188 = 0; _188 < 3; _188++)
    {
        if (color[_188] <= 0.003130800090730190277099609375)
        {
            color[_188] *= 12.9200000762939453125;
        }
        else
        {
            color[_188] = (1.05499994754791259765625 * powr(color[_188], 0.4166666567325592041015625)) - 0.054999999701976776123046875;
        }
    }
    out.frag_color = float4(color.xyz * color.w, color.w);
    return out;
}

