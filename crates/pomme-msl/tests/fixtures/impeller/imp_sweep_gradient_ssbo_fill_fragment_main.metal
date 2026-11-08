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

struct imp_sweep_gradient_ssbo_fill_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_sweep_gradient_ssbo_fill_fragment_main_in
{
    float2 v_position [[user(locn0)]];
};

fragment imp_sweep_gradient_ssbo_fill_fragment_main_out imp_sweep_gradient_ssbo_fill_fragment_main(imp_sweep_gradient_ssbo_fill_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], const device ColorData& color_data [[buffer(1)]], float4 gl_FragCoord [[position]])
{
    imp_sweep_gradient_ssbo_fill_fragment_main_out out = {};
    float2 _171 = in.v_position - frag_info.center;
    float _191 = (((precise::atan2(-_171.y, -_171.x) * 0.15915493667125701904296875) + 0.5) + frag_info.bias0) * frag_info.scale;
    bool _198 = (_191 < 0.0) || (_191 > 1.0);
    bool _206;
    if (_198)
    {
        _206 = frag_info.tile_mode == 3.0;
    }
    else
    {
        _206 = _198;
    }
    float4 _450;
    if (_206)
    {
        _450 = frag_info.decal_border_color;
    }
    else
    {
        float _443;
        if (frag_info.tile_mode == 0.0)
        {
            _443 = fast::clamp(_191, 0.0, 1.0);
        }
        else
        {
            float _444;
            if (frag_info.tile_mode == 1.0)
            {
                _444 = fract(_191);
            }
            else
            {
                float _445;
                if (frag_info.tile_mode == 2.0)
                {
                    float _343 = _191 - 1.0;
                    _445 = abs((_343 - (2.0 * floor(_343 * 0.5))) - 1.0);
                }
                else
                {
                    _445 = _191;
                }
                _444 = _445;
            }
            _443 = _444;
        }
        float4 _451;
        int _446 = 1;
        for (;;)
        {
            if (_446 < frag_info.colors_length)
            {
                int _241 = _446 - 1;
                bool _260 = _443 >= color_data.colors[_241].stop;
                bool _267;
                if (_260)
                {
                    _267 = _443 <= color_data.colors[_446].stop;
                }
                else
                {
                    _267 = _260;
                }
                if (_267)
                {
                    float _275 = color_data.colors[_446].stop - color_data.colors[_241].stop;
                    float4 _458;
                    if (_275 < 0.001000000047497451305389404296875)
                    {
                        _458 = color_data.colors[_446].color;
                    }
                    else
                    {
                        _458 = mix(color_data.colors[_241].color, color_data.colors[_446].color, float4((_443 - color_data.colors[_241].stop) / _275));
                    }
                    _451 = _458;
                    break;
                }
                _446++;
                continue;
            }
            else
            {
                _451 = float4(0.0);
                break;
            }
        }
        _450 = _451;
    }
    out.frag_color = float4(_450.xyz * _450.w, _450.w) * frag_info.alpha;
    float4 _312 = out.frag_color;
    uint _379 = uint(gl_FragCoord.x) % 8u;
    uint _385 = uint(gl_FragCoord.y) ^ _379;
    float3 _418 = _312.xyz + float3(((float(((((((_385 & 1u) << uint(5)) | ((_379 & 1u) << uint(4))) | ((_385 & 2u) << uint(2))) | ((_379 & 2u) << uint(1))) | ((_385 & 4u) >> uint(1))) | ((_379 & 4u) >> uint(2))) * 0.015625) - 0.4921875) * 0.015625);
    float4 _437 = _312;
    _437.x = _418.x;
    _437.y = _418.y;
    _437.z = _418.z;
    out.frag_color = _437;
    return out;
}

