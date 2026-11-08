#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float2 start_point;
    float2 start_to_end;
    float alpha;
    float tile_mode;
    float colors_length;
    float inverse_dot_start_to_end;
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

struct imp_linear_gradient_uniform_fill_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_linear_gradient_uniform_fill_fragment_main_in
{
    float2 v_position [[user(locn0)]];
};

fragment imp_linear_gradient_uniform_fill_fragment_main_out imp_linear_gradient_uniform_fill_fragment_main(imp_linear_gradient_uniform_fill_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], constant StopPairsInfo& stop_pairs_info [[buffer(1)]], constant ColorsInfo& colors_info [[buffer(2)]])
{
    imp_linear_gradient_uniform_fill_fragment_main_out out = {};
    float _109 = dot(in.v_position - frag_info.start_point, frag_info.start_to_end) * frag_info.inverse_dot_start_to_end;
    bool _114 = (_109 < 0.0) || (_109 > 1.0);
    bool _122;
    if (_114)
    {
        _122 = frag_info.tile_mode == 3.0;
    }
    else
    {
        _122 = _114;
    }
    if (_122)
    {
        out.frag_color = frag_info.decal_border_color;
    }
    else
    {
        float _304;
        if (frag_info.tile_mode == 0.0)
        {
            _304 = fast::clamp(_109, 0.0, 1.0);
        }
        else
        {
            float _305;
            if (frag_info.tile_mode == 1.0)
            {
                _305 = fract(_109);
            }
            else
            {
                float _306;
                if (frag_info.tile_mode == 2.0)
                {
                    float _269 = _109 - 1.0;
                    _306 = abs((_269 - (2.0 * floor(_269 * 0.5))) - 1.0);
                }
                else
                {
                    _306 = _109;
                }
                _305 = _306;
            }
            _304 = _305;
        }
        float2 _314;
        _314 = stop_pairs_info.stop_pairs[0].xy;
        bool _182;
        float2 _309;
        int _307 = 1;
        bool _308 = false;
        for (; float(_307) < frag_info.colors_length; _314 = _309, _308 = _182, _307++)
        {
            if (_308)
            {
                _309 = stop_pairs_info.stop_pairs[_307 / 2].xy;
            }
            else
            {
                _309 = stop_pairs_info.stop_pairs[_307 / 2].zw;
            }
            _182 = !_308;
            bool _187 = _304 >= _314.x;
            bool _194;
            if (_187)
            {
                _194 = _304 <= _309.x;
            }
            else
            {
                _194 = _187;
            }
            if (_194)
            {
                if (_309.y > 1000.0)
                {
                    out.frag_color = colors_info.colors[_307];
                }
                else
                {
                    out.frag_color = mix(colors_info.colors[_307 - 1], colors_info.colors[_307], float4((_304 - _314.x) * _309.y));
                }
                break;
            }
        }
    }
    out.frag_color = float4(out.frag_color.xyz * out.frag_color.w, out.frag_color.w) * frag_info.alpha;
    return out;
}

