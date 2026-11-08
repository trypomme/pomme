#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float2 center;
    float2 focus;
    float focus_radius;
    float radius;
    float tile_mode;
    float alpha;
    float colors_length;
    float4 decal_border_color;
};

struct StopPairsInfo
{
    float4 stop_pairs[128];
};

struct ColorsInfo
{
    float4 colors[256];
};

struct imp_conical_gradient_uniform_fill_radial_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_conical_gradient_uniform_fill_radial_fragment_main_in
{
    float2 v_position [[user(locn0)]];
};

fragment imp_conical_gradient_uniform_fill_radial_fragment_main_out imp_conical_gradient_uniform_fill_radial_fragment_main(imp_conical_gradient_uniform_fill_radial_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], constant StopPairsInfo& stop_pairs_info [[buffer(1)]], constant ColorsInfo& colors_info [[buffer(2)]])
{
    imp_conical_gradient_uniform_fill_radial_fragment_main_out out = {};
    float _325 = frag_info.radius - frag_info.focus_radius;
    float _343 = (length((in.v_position - frag_info.focus) * (1.0 / _325)) * sign(_325)) - (frag_info.focus_radius / _325);
    bool _370 = (_343 < 0.0) || (_343 > 1.0);
    bool _376;
    if (_370)
    {
        _376 = frag_info.tile_mode == 3.0;
    }
    else
    {
        _376 = _370;
    }
    float4 _539;
    if (_376)
    {
        _539 = frag_info.decal_border_color;
    }
    else
    {
        float _524;
        if (frag_info.tile_mode == 0.0)
        {
            _524 = fast::clamp(_343, 0.0, 1.0);
        }
        else
        {
            float _525;
            if (frag_info.tile_mode == 1.0)
            {
                _525 = fract(_343);
            }
            else
            {
                float _526;
                if (frag_info.tile_mode == 2.0)
                {
                    float _486 = _343 - 1.0;
                    _526 = abs((_486 - (2.0 * floor(_486 * 0.5))) - 1.0);
                }
                else
                {
                    _526 = _343;
                }
                _525 = _526;
            }
            _524 = _525;
        }
        float2 _534;
        _534 = stop_pairs_info.stop_pairs[0].xy;
        bool _414;
        float2 _529;
        float4 _540;
        int _527 = 1;
        bool _528 = false;
        for (;;)
        {
            if (float(_527) < frag_info.colors_length)
            {
                if (_528)
                {
                    _529 = stop_pairs_info.stop_pairs[_527 / 2].xy;
                }
                else
                {
                    _529 = stop_pairs_info.stop_pairs[_527 / 2].zw;
                }
                _414 = !_528;
                bool _418 = _524 >= _534.x;
                bool _425;
                if (_418)
                {
                    _425 = _524 <= _529.x;
                }
                else
                {
                    _425 = _418;
                }
                if (_425)
                {
                    float4 _549;
                    if (_529.y > 1000.0)
                    {
                        _549 = colors_info.colors[_527];
                    }
                    else
                    {
                        _549 = mix(colors_info.colors[_527 - 1], colors_info.colors[_527], float4((_524 - _534.x) * _529.y));
                    }
                    _540 = _549;
                    break;
                }
                _534 = _529;
                _528 = _414;
                _527++;
                continue;
            }
            else
            {
                _540 = float4(0.0);
                break;
            }
        }
        _539 = _540;
    }
    out.frag_color = float4(_539.xyz * _539.w, _539.w) * frag_info.alpha;
    return out;
}

