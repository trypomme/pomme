#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float4 input_alpha_output_alpha_tmx_tmy;
    float4 source_rect;
    float use_strict_source_rect;
};

constant float supports_decal_tmp [[function_constant(0)]];
constant float supports_decal = is_function_constant_defined(supports_decal_tmp) ? supports_decal_tmp : 1.0;
constant float src_coeff_tmp [[function_constant(1)]];
constant float src_coeff = is_function_constant_defined(src_coeff_tmp) ? src_coeff_tmp : 1.0;
constant float src_coeff_dst_alpha_tmp [[function_constant(2)]];
constant float src_coeff_dst_alpha = is_function_constant_defined(src_coeff_dst_alpha_tmp) ? src_coeff_dst_alpha_tmp : 1.0;
constant float dst_coeff_tmp [[function_constant(3)]];
constant float dst_coeff = is_function_constant_defined(dst_coeff_tmp) ? dst_coeff_tmp : 1.0;
constant float dst_coeff_src_alpha_tmp [[function_constant(4)]];
constant float dst_coeff_src_alpha = is_function_constant_defined(dst_coeff_src_alpha_tmp) ? dst_coeff_src_alpha_tmp : 1.0;
constant float dst_coeff_src_color_tmp [[function_constant(5)]];
constant float dst_coeff_src_color = is_function_constant_defined(dst_coeff_src_color_tmp) ? dst_coeff_src_color_tmp : 1.0;

struct imp_porter_duff_blend_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_porter_duff_blend_fragment_main_in
{
    float2 v_texture_coords [[user(locn0)]];
    float4 v_color [[user(locn1)]];
};

fragment imp_porter_duff_blend_fragment_main_out imp_porter_duff_blend_fragment_main(imp_porter_duff_blend_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], texture2d<float> texture_sampler_dst [[texture(0)]], sampler texture_sampler_dstSmplr [[sampler(0)]])
{
    imp_porter_duff_blend_fragment_main_out out = {};
    float2 _148 = mix(in.v_texture_coords, float2(fast::clamp(in.v_texture_coords.x, frag_info.source_rect.x, frag_info.source_rect.z), fast::clamp(in.v_texture_coords.y, frag_info.source_rect.y, frag_info.source_rect.w)), float2(frag_info.use_strict_source_rect));
    float4 _281;
    do
    {
        if (supports_decal > 0.0)
        {
            _281 = texture_sampler_dst.sample(texture_sampler_dstSmplr, _148);
            break;
        }
        float4 _280;
        do
        {
            bool _234 = frag_info.input_alpha_output_alpha_tmx_tmy.z == 3.0;
            bool _247;
            if (_234)
            {
                float _237 = _148.x;
                bool _238 = _237 < 0.0;
                bool _245;
                if (!_238)
                {
                    _245 = _237 >= 1.0;
                }
                else
                {
                    _245 = _238;
                }
                _247 = _245;
            }
            else
            {
                _247 = _234;
            }
            bool _266;
            if (!_247)
            {
                bool _251 = frag_info.input_alpha_output_alpha_tmx_tmy.w == 3.0;
                bool _264;
                if (_251)
                {
                    float _254 = _148.y;
                    bool _255 = _254 < 0.0;
                    bool _262;
                    if (!_255)
                    {
                        _262 = _254 >= 1.0;
                    }
                    else
                    {
                        _262 = _255;
                    }
                    _264 = _262;
                }
                else
                {
                    _264 = _251;
                }
                _266 = _264;
            }
            else
            {
                _266 = _247;
            }
            if (_266)
            {
                _280 = float4(0.0);
                break;
            }
            _280 = texture_sampler_dst.sample(texture_sampler_dstSmplr, _148, bias(-0.4749999940395355224609375));
            break;
        } while(false);
        _281 = _280;
        break;
    } while(false);
    float4 _156 = _281 * frag_info.input_alpha_output_alpha_tmx_tmy.x;
    out.frag_color = (in.v_color * (src_coeff + (_156.w * src_coeff_dst_alpha))) + (_156 * (float4(dst_coeff + (in.v_color.w * dst_coeff_src_alpha)) + (in.v_color * dst_coeff_src_color)));
    out.frag_color *= frag_info.input_alpha_output_alpha_tmx_tmy.y;
    return out;
}

