#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

constant float supports_decal_tmp [[function_constant(0)]];
constant float supports_decal = is_function_constant_defined(supports_decal_tmp) ? supports_decal_tmp : 1.0;

struct FragInfo
{
    float x_tile_mode;
    float y_tile_mode;
    float alpha;
};

struct imp_tiled_texture_fill_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_tiled_texture_fill_fragment_main_in
{
    float2 v_texture_coords [[user(locn0)]];
};

fragment imp_tiled_texture_fill_fragment_main_out imp_tiled_texture_fill_fragment_main(imp_tiled_texture_fill_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], texture2d<float> texture_sampler [[texture(0)]], sampler texture_samplerSmplr [[sampler(0)]])
{
    imp_tiled_texture_fill_fragment_main_out out = {};
    if (supports_decal == 1.0)
    {
        out.frag_color = texture_sampler.sample(texture_samplerSmplr, in.v_texture_coords, bias(-0.4749999940395355224609375)) * frag_info.alpha;
    }
    else
    {
        float4 _180;
        do
        {
            bool _136 = frag_info.x_tile_mode == 3.0;
            bool _149;
            if (_136)
            {
                bool _140 = in.v_texture_coords.x < 0.0;
                bool _147;
                if (!_140)
                {
                    _147 = in.v_texture_coords.x >= 1.0;
                }
                else
                {
                    _147 = _140;
                }
                _149 = _147;
            }
            else
            {
                _149 = _136;
            }
            bool _168;
            if (!_149)
            {
                bool _153 = frag_info.y_tile_mode == 3.0;
                bool _166;
                if (_153)
                {
                    bool _157 = in.v_texture_coords.y < 0.0;
                    bool _164;
                    if (!_157)
                    {
                        _164 = in.v_texture_coords.y >= 1.0;
                    }
                    else
                    {
                        _164 = _157;
                    }
                    _166 = _164;
                }
                else
                {
                    _166 = _153;
                }
                _168 = _166;
            }
            else
            {
                _168 = _149;
            }
            if (_168)
            {
                _180 = float4(0.0);
                break;
            }
            _180 = texture_sampler.sample(texture_samplerSmplr, in.v_texture_coords, bias(-0.4749999940395355224609375));
            break;
        } while(false);
        out.frag_color = _180 * frag_info.alpha;
    }
    return out;
}

