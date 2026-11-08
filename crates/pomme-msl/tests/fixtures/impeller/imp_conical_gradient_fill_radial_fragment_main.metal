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
    float2 focus;
    float focus_radius;
};

struct imp_conical_gradient_fill_radial_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_conical_gradient_fill_radial_fragment_main_in
{
    float2 v_position [[user(locn0)]];
};

fragment imp_conical_gradient_fill_radial_fragment_main_out imp_conical_gradient_fill_radial_fragment_main(imp_conical_gradient_fill_radial_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], texture2d<float> texture_sampler [[texture(0)]], sampler texture_samplerSmplr [[sampler(0)]])
{
    imp_conical_gradient_fill_radial_fragment_main_out out = {};
    float _378 = frag_info.radius - frag_info.focus_radius;
    float _396 = (length((in.v_position - frag_info.focus) * (1.0 / _378)) * sign(_378)) - (frag_info.focus_radius / _378);
    float4 _653;
    do
    {
        bool _459 = frag_info.tile_mode == 3.0;
        bool _472;
        if (_459)
        {
            bool _463 = _396 < 0.0;
            bool _470;
            if (!_463)
            {
                _470 = _396 >= 1.0;
            }
            else
            {
                _470 = _463;
            }
            _472 = _470;
        }
        else
        {
            _472 = _459;
        }
        bool _491;
        if (!_472)
        {
            _491 = _459 ? false : _459;
        }
        else
        {
            _491 = _472;
        }
        if (_491)
        {
            _653 = frag_info.decal_border_color;
            break;
        }
        bool _523 = frag_info.tile_mode == 0.0;
        float _647;
        if (_523)
        {
            _647 = fast::clamp(_396, 0.0, 1.0);
        }
        else
        {
            float _648;
            if (frag_info.tile_mode == 1.0)
            {
                _648 = fract(_396);
            }
            else
            {
                float _649;
                if (frag_info.tile_mode == 2.0)
                {
                    float _538 = _396 - 1.0;
                    _649 = abs((_538 - (2.0 * floor(_538 * 0.5))) - 1.0);
                }
                else
                {
                    _649 = _396;
                }
                _648 = _649;
            }
            _647 = _648;
        }
        float _650;
        if (_523)
        {
            _650 = 0.5;
        }
        else
        {
            float _651;
            if (frag_info.tile_mode == 1.0)
            {
                _651 = fract(0.5);
            }
            else
            {
                float _652;
                if (frag_info.tile_mode == 2.0)
                {
                    _652 = abs((-1.5) - (2.0 * floor(-0.25)));
                }
                else
                {
                    _652 = 0.5;
                }
                _651 = _652;
            }
            _650 = _651;
        }
        _653 = texture_sampler.sample(texture_samplerSmplr, float2(mix(frag_info.half_texel.x, 1.0 - frag_info.half_texel.x, _647), mix(frag_info.half_texel.y, 1.0 - frag_info.half_texel.y, _650)), bias(-0.4749999940395355224609375));
        break;
    } while(false);
    out.frag_color = float4(_653.xyz * _653.w, _653.w) * frag_info.alpha;
    return out;
}

