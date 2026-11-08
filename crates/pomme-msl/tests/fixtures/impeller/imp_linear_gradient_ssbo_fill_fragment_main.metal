#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float2 start_point;
    float2 end_point;
    float alpha;
    float tile_mode;
    float4 decal_border_color;
    int colors_length;
    float2 start_to_end;
    float inverse_dot_start_to_end;
};

struct ColorPoint
{
    float4 color;
    float stop;
    float inverse_delta;
    char _m0_final_padding[8];
};

struct ColorData
{
    ColorPoint colors[1];
};

struct imp_linear_gradient_ssbo_fill_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_linear_gradient_ssbo_fill_fragment_main_in
{
    float2 v_position [[user(locn0)]];
};

fragment imp_linear_gradient_ssbo_fill_fragment_main_out imp_linear_gradient_ssbo_fill_fragment_main(imp_linear_gradient_ssbo_fill_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], const device ColorData& color_data [[buffer(1)]], float4 gl_FragCoord [[position]])
{
    imp_linear_gradient_ssbo_fill_fragment_main_out out = {};
    float _180 = dot(in.v_position - frag_info.start_point, frag_info.start_to_end) * frag_info.inverse_dot_start_to_end;
    bool _185 = (_180 < 0.0) || (_180 > 1.0);
    bool _193;
    if (_185)
    {
        _193 = frag_info.tile_mode == 3.0;
    }
    else
    {
        _193 = _185;
    }
    if (_193)
    {
        out.frag_color = frag_info.decal_border_color;
    }
    else
    {
        float _430;
        if (frag_info.tile_mode == 0.0)
        {
            _430 = fast::clamp(_180, 0.0, 1.0);
        }
        else
        {
            float _431;
            if (frag_info.tile_mode == 1.0)
            {
                _431 = fract(_180);
            }
            else
            {
                float _432;
                if (frag_info.tile_mode == 2.0)
                {
                    float _330 = _180 - 1.0;
                    _432 = abs((_330 - (2.0 * floor(_330 * 0.5))) - 1.0);
                }
                else
                {
                    _432 = _180;
                }
                _431 = _432;
            }
            _430 = _431;
        }
        for (int _433 = 1; _433 < frag_info.colors_length; _433++)
        {
            int _229 = _433 - 1;
            bool _252 = _430 >= color_data.colors[_229].stop;
            bool _259;
            if (_252)
            {
                _259 = _430 <= color_data.colors[_433].stop;
            }
            else
            {
                _259 = _252;
            }
            if (_259)
            {
                if (color_data.colors[_433].inverse_delta > 1000.0)
                {
                    out.frag_color = color_data.colors[_433].color;
                }
                else
                {
                    out.frag_color = mix(color_data.colors[_229].color, color_data.colors[_433].color, float4((_430 - color_data.colors[_229].stop) * color_data.colors[_433].inverse_delta));
                }
                break;
            }
        }
    }
    out.frag_color = float4(out.frag_color.xyz * out.frag_color.w, out.frag_color.w) * frag_info.alpha;
    float4 _298 = out.frag_color;
    uint _366 = uint(gl_FragCoord.x) % 8u;
    uint _372 = uint(gl_FragCoord.y) ^ _366;
    float3 _405 = _298.xyz + float3(((float(((((((_372 & 1u) << uint(5)) | ((_366 & 1u) << uint(4))) | ((_372 & 2u) << uint(2))) | ((_366 & 2u) << uint(1))) | ((_372 & 4u) >> uint(1))) | ((_366 & 4u) >> uint(2))) * 0.015625) - 0.4921875) * 0.015625);
    float4 _424 = _298;
    _424.x = _405.x;
    _424.y = _405.y;
    _424.z = _405.z;
    out.frag_color = _424;
    return out;
}

