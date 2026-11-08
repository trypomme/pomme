#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float is_color_glyph;
    float use_text_color;
    float4 text_color;
    float text_contrast;
};

constant float use_alpha_color_channel_tmp [[function_constant(0)]];
constant float use_alpha_color_channel = is_function_constant_defined(use_alpha_color_channel_tmp) ? use_alpha_color_channel_tmp : 1.0;

struct imp_glyph_atlas_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_glyph_atlas_fragment_main_in
{
    float2 v_uv [[user(locn0)]];
};

fragment imp_glyph_atlas_fragment_main_out imp_glyph_atlas_fragment_main(imp_glyph_atlas_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], texture2d<float> glyph_atlas_sampler [[texture(0)]], sampler glyph_atlas_samplerSmplr [[sampler(0)]])
{
    imp_glyph_atlas_fragment_main_out out = {};
    float4 _21 = glyph_atlas_sampler.sample(glyph_atlas_samplerSmplr, in.v_uv);
    if (frag_info.is_color_glyph == 1.0)
    {
        if (frag_info.use_text_color == 1.0)
        {
            out.frag_color = _21.wwww * frag_info.text_color;
        }
        else
        {
            out.frag_color = _21 * frag_info.text_color.wwww;
        }
    }
    else
    {
        float _90;
        if (use_alpha_color_channel == 1.0)
        {
            _90 = _21.w;
        }
        else
        {
            _90 = _21.x;
        }
        float _91;
        if (frag_info.text_contrast != 1.0)
        {
            _91 = 1.0 - powr(1.0 - _90, frag_info.text_contrast);
        }
        else
        {
            _91 = _90;
        }
        out.frag_color = float4(_91) * frag_info.text_color;
    }
    return out;
}

