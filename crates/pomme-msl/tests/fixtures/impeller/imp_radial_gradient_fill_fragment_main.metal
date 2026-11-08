#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float2 center;
    float radius;
    float tile_mode;
    float4 decal_border_color;
    float alpha;
    float2 half_texel;
};

struct imp_radial_gradient_fill_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_radial_gradient_fill_fragment_main_in
{
    float2 v_position [[user(locn0)]];
};

fragment imp_radial_gradient_fill_fragment_main_out imp_radial_gradient_fill_fragment_main(imp_radial_gradient_fill_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], texture2d<float> texture_sampler [[texture(0)]], sampler texture_samplerSmplr [[sampler(0)]])
{
    imp_radial_gradient_fill_fragment_main_out out = {};
    float _251 = length(in.v_position - frag_info.center) / frag_info.radius;
    float4 _509;
    do
    {
        bool _317 = frag_info.tile_mode == 3.0;
        bool _330;
        if (_317)
        {
            bool _321 = _251 < 0.0;
            bool _328;
            if (!_321)
            {
                _328 = _251 >= 1.0;
            }
            else
            {
                _328 = _321;
            }
            _330 = _328;
        }
        else
        {
            _330 = _317;
        }
        bool _349;
        if (!_330)
        {
            _349 = _317 ? false : _317;
        }
        else
        {
            _349 = _330;
        }
        if (_349)
        {
            _509 = frag_info.decal_border_color;
            break;
        }
        bool _381 = frag_info.tile_mode == 0.0;
        float _503;
        if (_381)
        {
            _503 = fast::clamp(_251, 0.0, 1.0);
        }
        else
        {
            float _504;
            if (frag_info.tile_mode == 1.0)
            {
                _504 = fract(_251);
            }
            else
            {
                float _505;
                if (frag_info.tile_mode == 2.0)
                {
                    float _396 = _251 - 1.0;
                    _505 = abs((_396 - (2.0 * floor(_396 * 0.5))) - 1.0);
                }
                else
                {
                    _505 = _251;
                }
                _504 = _505;
            }
            _503 = _504;
        }
        float _506;
        if (_381)
        {
            _506 = 0.5;
        }
        else
        {
            float _507;
            if (frag_info.tile_mode == 1.0)
            {
                _507 = fract(0.5);
            }
            else
            {
                float _508;
                if (frag_info.tile_mode == 2.0)
                {
                    _508 = abs((-1.5) - (2.0 * floor(-0.25)));
                }
                else
                {
                    _508 = 0.5;
                }
                _507 = _508;
            }
            _506 = _507;
        }
        _509 = texture_sampler.sample(texture_samplerSmplr, float2(mix(frag_info.half_texel.x, 1.0 - frag_info.half_texel.x, _503), mix(frag_info.half_texel.y, 1.0 - frag_info.half_texel.y, _506)), bias(-0.4749999940395355224609375));
        break;
    } while(false);
    out.frag_color = _509;
    out.frag_color = float4(out.frag_color.xyz * out.frag_color.w, out.frag_color.w) * frag_info.alpha;
    return out;
}

