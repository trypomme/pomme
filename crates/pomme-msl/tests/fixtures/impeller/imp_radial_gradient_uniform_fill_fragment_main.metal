#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float2 center;
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

struct imp_radial_gradient_uniform_fill_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_radial_gradient_uniform_fill_fragment_main_in
{
    float2 v_position [[user(locn0)]];
};

fragment imp_radial_gradient_uniform_fill_fragment_main_out imp_radial_gradient_uniform_fill_fragment_main(imp_radial_gradient_uniform_fill_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], constant StopPairsInfo& stop_pairs_info [[buffer(1)]], constant ColorsInfo& colors_info [[buffer(2)]])
{
    imp_radial_gradient_uniform_fill_fragment_main_out out = {};
    float _105 = length(in.v_position - frag_info.center) / frag_info.radius;
    bool _112 = (_105 < 0.0) || (_105 > 1.0);
    bool _120;
    if (_112)
    {
        _120 = frag_info.tile_mode == 3.0;
    }
    else
    {
        _120 = _112;
    }
    float4 _318;
    if (_120)
    {
        _318 = frag_info.decal_border_color;
    }
    else
    {
        float _303;
        if (frag_info.tile_mode == 0.0)
        {
            _303 = fast::clamp(_105, 0.0, 1.0);
        }
        else
        {
            float _304;
            if (frag_info.tile_mode == 1.0)
            {
                _304 = fract(_105);
            }
            else
            {
                float _305;
                if (frag_info.tile_mode == 2.0)
                {
                    float _268 = _105 - 1.0;
                    _305 = abs((_268 - (2.0 * floor(_268 * 0.5))) - 1.0);
                }
                else
                {
                    _305 = _105;
                }
                _304 = _305;
            }
            _303 = _304;
        }
        float2 _313;
        _313 = stop_pairs_info.stop_pairs[0].xy;
        bool _178;
        float2 _308;
        float4 _319;
        int _306 = 1;
        bool _307 = false;
        for (;;)
        {
            if (float(_306) < frag_info.colors_length)
            {
                if (_307)
                {
                    _308 = stop_pairs_info.stop_pairs[_306 / 2].xy;
                }
                else
                {
                    _308 = stop_pairs_info.stop_pairs[_306 / 2].zw;
                }
                _178 = !_307;
                bool _183 = _303 >= _313.x;
                bool _190;
                if (_183)
                {
                    _190 = _303 <= _308.x;
                }
                else
                {
                    _190 = _183;
                }
                if (_190)
                {
                    float4 _327;
                    if (_308.y > 1000.0)
                    {
                        _327 = colors_info.colors[_306];
                    }
                    else
                    {
                        _327 = mix(colors_info.colors[_306 - 1], colors_info.colors[_306], float4((_303 - _313.x) * _308.y));
                    }
                    _319 = _327;
                    break;
                }
                _313 = _308;
                _307 = _178;
                _306++;
                continue;
            }
            else
            {
                _319 = float4(0.0);
                break;
            }
        }
        _318 = _319;
    }
    out.frag_color = float4(_318.xyz * _318.w, _318.w) * frag_info.alpha;
    return out;
}

