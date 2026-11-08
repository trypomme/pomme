#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float2 center;
    float bias0;
    float scale;
    float tile_mode;
    float4 decal_border_color;
    float alpha;
    float2 half_texel;
};

struct imp_sweep_gradient_fill_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_sweep_gradient_fill_fragment_main_in
{
    float2 v_position [[user(locn0)]];
};

fragment imp_sweep_gradient_fill_fragment_main_out imp_sweep_gradient_fill_fragment_main(imp_sweep_gradient_fill_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], texture2d<float> texture_sampler [[texture(0)]], sampler texture_samplerSmplr [[sampler(0)]])
{
    imp_sweep_gradient_fill_fragment_main_out out = {};
    float2 _243 = in.v_position - frag_info.center;
    float _265 = (((precise::atan2(-_243.y, -_243.x) * 0.15915493667125701904296875) + 0.5) + frag_info.bias0) * frag_info.scale;
    float4 _524;
    do
    {
        bool _330 = frag_info.tile_mode == 3.0;
        bool _343;
        if (_330)
        {
            bool _334 = _265 < 0.0;
            bool _341;
            if (!_334)
            {
                _341 = _265 >= 1.0;
            }
            else
            {
                _341 = _334;
            }
            _343 = _341;
        }
        else
        {
            _343 = _330;
        }
        bool _362;
        if (!_343)
        {
            _362 = _330 ? false : _330;
        }
        else
        {
            _362 = _343;
        }
        if (_362)
        {
            _524 = frag_info.decal_border_color;
            break;
        }
        bool _394 = frag_info.tile_mode == 0.0;
        float _518;
        if (_394)
        {
            _518 = fast::clamp(_265, 0.0, 1.0);
        }
        else
        {
            float _519;
            if (frag_info.tile_mode == 1.0)
            {
                _519 = fract(_265);
            }
            else
            {
                float _520;
                if (frag_info.tile_mode == 2.0)
                {
                    float _409 = _265 - 1.0;
                    _520 = abs((_409 - (2.0 * floor(_409 * 0.5))) - 1.0);
                }
                else
                {
                    _520 = _265;
                }
                _519 = _520;
            }
            _518 = _519;
        }
        float _521;
        if (_394)
        {
            _521 = 0.5;
        }
        else
        {
            float _522;
            if (frag_info.tile_mode == 1.0)
            {
                _522 = fract(0.5);
            }
            else
            {
                float _523;
                if (frag_info.tile_mode == 2.0)
                {
                    _523 = abs((-1.5) - (2.0 * floor(-0.25)));
                }
                else
                {
                    _523 = 0.5;
                }
                _522 = _523;
            }
            _521 = _522;
        }
        _524 = texture_sampler.sample(texture_samplerSmplr, float2(mix(frag_info.half_texel.x, 1.0 - frag_info.half_texel.x, _518), mix(frag_info.half_texel.y, 1.0 - frag_info.half_texel.y, _521)), bias(-0.4749999940395355224609375));
        break;
    } while(false);
    out.frag_color = _524;
    out.frag_color = float4(out.frag_color.xyz * out.frag_color.w, out.frag_color.w) * frag_info.alpha;
    return out;
}

