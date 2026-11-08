#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float4x4 color_m;
    float4 color_v;
    float input_alpha;
    float output_alpha;
};

struct imp_color_matrix_color_filter_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_color_matrix_color_filter_fragment_main_in
{
    float2 v_texture_coords [[user(locn0)]];
};

fragment imp_color_matrix_color_filter_fragment_main_out imp_color_matrix_color_filter_fragment_main(imp_color_matrix_color_filter_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], texture2d<float> input_texture [[texture(0)]], sampler input_textureSmplr [[sampler(0)]])
{
    imp_color_matrix_color_filter_fragment_main_out out = {};
    float4 _73 = input_texture.sample(input_textureSmplr, in.v_texture_coords);
    float4 _83 = _73 * frag_info.input_alpha;
    float4 _177;
    do
    {
        float _141 = _83.w;
        if (_141 == 0.0)
        {
            _177 = float4(0.0);
            break;
        }
        _177 = float4(_83.xyz / float3(_141), _141);
        break;
    } while(false);
    float4 _107 = fast::clamp((frag_info.color_m * _177) + frag_info.color_v, float4(0.0), float4(1.0));
    float _164 = _107.w;
    out.frag_color = float4(_107.xyz * _164, _164) * frag_info.output_alpha;
    return out;
}

