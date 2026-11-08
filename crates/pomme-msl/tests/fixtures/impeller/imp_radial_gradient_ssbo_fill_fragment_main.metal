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
    int colors_length;
};

struct ColorPoint
{
    float4 color;
    float stop;
    char _m0_final_padding[12];
};

struct ColorData
{
    ColorPoint colors[1];
};

struct imp_radial_gradient_ssbo_fill_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_radial_gradient_ssbo_fill_fragment_main_in
{
    float2 v_position [[user(locn0)]];
};

fragment imp_radial_gradient_ssbo_fill_fragment_main_out imp_radial_gradient_ssbo_fill_fragment_main(imp_radial_gradient_ssbo_fill_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], const device ColorData& color_data [[buffer(1)]], float4 gl_FragCoord [[position]])
{
    imp_radial_gradient_ssbo_fill_fragment_main_out out = {};
    float _178 = length(in.v_position - frag_info.center) / frag_info.radius;
    bool _185 = (_178 < 0.0) || (_178 > 1.0);
    bool _192;
    if (_185)
    {
        _192 = frag_info.tile_mode == 3.0;
    }
    else
    {
        _192 = _185;
    }
    float4 _435;
    if (_192)
    {
        _435 = frag_info.decal_border_color;
    }
    else
    {
        float _428;
        if (frag_info.tile_mode == 0.0)
        {
            _428 = fast::clamp(_178, 0.0, 1.0);
        }
        else
        {
            float _429;
            if (frag_info.tile_mode == 1.0)
            {
                _429 = fract(_178);
            }
            else
            {
                float _430;
                if (frag_info.tile_mode == 2.0)
                {
                    float _330 = _178 - 1.0;
                    _430 = abs((_330 - (2.0 * floor(_330 * 0.5))) - 1.0);
                }
                else
                {
                    _430 = _178;
                }
                _429 = _430;
            }
            _428 = _429;
        }
        float4 _436;
        int _431 = 1;
        for (;;)
        {
            if (_431 < frag_info.colors_length)
            {
                int _227 = _431 - 1;
                bool _246 = _428 >= color_data.colors[_227].stop;
                bool _253;
                if (_246)
                {
                    _253 = _428 <= color_data.colors[_431].stop;
                }
                else
                {
                    _253 = _246;
                }
                if (_253)
                {
                    float _261 = color_data.colors[_431].stop - color_data.colors[_227].stop;
                    float4 _443;
                    if (_261 < 0.001000000047497451305389404296875)
                    {
                        _443 = color_data.colors[_431].color;
                    }
                    else
                    {
                        _443 = mix(color_data.colors[_227].color, color_data.colors[_431].color, float4((_428 - color_data.colors[_227].stop) / _261));
                    }
                    _436 = _443;
                    break;
                }
                _431++;
                continue;
            }
            else
            {
                _436 = float4(0.0);
                break;
            }
        }
        _435 = _436;
    }
    out.frag_color = float4(_435.xyz * _435.w, _435.w) * frag_info.alpha;
    float4 _298 = out.frag_color;
    uint _366 = uint(gl_FragCoord.x) % 8u;
    uint _372 = uint(gl_FragCoord.y) ^ _366;
    float3 _405 = _298.xyz + float3(((float(((((((_372 & 1u) << uint(5)) | ((_366 & 1u) << uint(4))) | ((_372 & 2u) << uint(2))) | ((_366 & 2u) << uint(1))) | ((_372 & 4u) >> uint(1))) | ((_366 & 4u) >> uint(2))) * 0.015625) - 0.4921875) * 0.015625);
    float4 _422 = _298;
    _422.x = _405.x;
    _422.y = _405.y;
    _422.z = _405.z;
    out.frag_color = _422;
    return out;
}

