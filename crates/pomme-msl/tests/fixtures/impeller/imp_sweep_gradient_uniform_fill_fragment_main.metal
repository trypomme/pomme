#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float2 center;
    float bias0;
    float scale;
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

struct imp_sweep_gradient_uniform_fill_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_sweep_gradient_uniform_fill_fragment_main_in
{
    float2 v_position [[user(locn0)]];
};

fragment imp_sweep_gradient_uniform_fill_fragment_main_out imp_sweep_gradient_uniform_fill_fragment_main(imp_sweep_gradient_uniform_fill_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], constant StopPairsInfo& stop_pairs_info [[buffer(1)]], constant ColorsInfo& colors_info [[buffer(2)]])
{
    imp_sweep_gradient_uniform_fill_fragment_main_out out = {};
    float2 _98 = in.v_position - frag_info.center;
    float _122 = (((precise::atan2(-_98.y, -_98.x) * 0.15915493667125701904296875) + 0.5) + frag_info.bias0) * frag_info.scale;
    bool _129 = (_122 < 0.0) || (_122 > 1.0);
    bool _137;
    if (_129)
    {
        _137 = frag_info.tile_mode == 3.0;
    }
    else
    {
        _137 = _129;
    }
    float4 _333;
    if (_137)
    {
        _333 = frag_info.decal_border_color;
    }
    else
    {
        float _318;
        if (frag_info.tile_mode == 0.0)
        {
            _318 = fast::clamp(_122, 0.0, 1.0);
        }
        else
        {
            float _319;
            if (frag_info.tile_mode == 1.0)
            {
                _319 = fract(_122);
            }
            else
            {
                float _320;
                if (frag_info.tile_mode == 2.0)
                {
                    float _281 = _122 - 1.0;
                    _320 = abs((_281 - (2.0 * floor(_281 * 0.5))) - 1.0);
                }
                else
                {
                    _320 = _122;
                }
                _319 = _320;
            }
            _318 = _319;
        }
        float2 _328;
        _328 = stop_pairs_info.stop_pairs[0].xy;
        bool _194;
        float2 _323;
        float4 _334;
        int _321 = 1;
        bool _322 = false;
        for (;;)
        {
            if (float(_321) < frag_info.colors_length)
            {
                if (_322)
                {
                    _323 = stop_pairs_info.stop_pairs[_321 / 2].xy;
                }
                else
                {
                    _323 = stop_pairs_info.stop_pairs[_321 / 2].zw;
                }
                _194 = !_322;
                bool _198 = _318 >= _328.x;
                bool _205;
                if (_198)
                {
                    _205 = _318 <= _323.x;
                }
                else
                {
                    _205 = _198;
                }
                if (_205)
                {
                    float4 _342;
                    if (_323.y > 1000.0)
                    {
                        _342 = colors_info.colors[_321];
                    }
                    else
                    {
                        _342 = mix(colors_info.colors[_321 - 1], colors_info.colors[_321], float4((_318 - _328.x) * _323.y));
                    }
                    _334 = _342;
                    break;
                }
                _328 = _323;
                _322 = _194;
                _321++;
                continue;
            }
            else
            {
                _334 = float4(0.0);
                break;
            }
        }
        _333 = _334;
    }
    out.frag_color = float4(_333.xyz * _333.w, _333.w) * frag_info.alpha;
    return out;
}

