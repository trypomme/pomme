#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

constant float supports_decal_tmp [[function_constant(0)]];
constant float supports_decal = is_function_constant_defined(supports_decal_tmp) ? supports_decal_tmp : 1.0;

struct FragInfo
{
    float alpha;
    float blend_mode;
    float tmx;
    float tmy;
};

struct imp_vertices_uber_1_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_vertices_uber_1_fragment_main_in
{
    float2 v_texture_coords [[user(locn0)]];
    float4 v_color [[user(locn1)]];
};

fragment imp_vertices_uber_1_fragment_main_out imp_vertices_uber_1_fragment_main(imp_vertices_uber_1_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], texture2d<float> texture_sampler [[texture(0)]], sampler texture_samplerSmplr [[sampler(0)]])
{
    imp_vertices_uber_1_fragment_main_out out = {};
    float4 _1131;
    do
    {
        if (in.v_color.w == 0.0)
        {
            _1131 = float4(0.0);
            break;
        }
        _1131 = float4(in.v_color.xyz / float3(in.v_color.w), in.v_color.w);
        break;
    } while(false);
    float4 _1133;
    do
    {
        if (supports_decal > 0.0)
        {
            _1133 = texture_sampler.sample(texture_samplerSmplr, in.v_texture_coords);
            break;
        }
        float4 _1132;
        do
        {
            bool _630 = frag_info.tmx == 3.0;
            bool _643;
            if (_630)
            {
                bool _634 = in.v_texture_coords.x < 0.0;
                bool _641;
                if (!_634)
                {
                    _641 = in.v_texture_coords.x >= 1.0;
                }
                else
                {
                    _641 = _634;
                }
                _643 = _641;
            }
            else
            {
                _643 = _630;
            }
            bool _662;
            if (!_643)
            {
                bool _647 = frag_info.tmy == 3.0;
                bool _660;
                if (_647)
                {
                    bool _651 = in.v_texture_coords.y < 0.0;
                    bool _658;
                    if (!_651)
                    {
                        _658 = in.v_texture_coords.y >= 1.0;
                    }
                    else
                    {
                        _658 = _651;
                    }
                    _660 = _658;
                }
                else
                {
                    _660 = _647;
                }
                _662 = _660;
            }
            else
            {
                _662 = _643;
            }
            if (_662)
            {
                _1132 = float4(0.0);
                break;
            }
            _1132 = texture_sampler.sample(texture_samplerSmplr, in.v_texture_coords, bias(-0.4749999940395355224609375));
            break;
        } while(false);
        _1133 = _1132;
        break;
    } while(false);
    float4 _1134;
    do
    {
        if (_1133.w == 0.0)
        {
            _1134 = float4(0.0);
            break;
        }
        _1134 = float4(_1133.xyz / float3(_1133.w), _1133.w);
        break;
    } while(false);
    int _530 = int(frag_info.blend_mode - 14.0);
    float3 _1147;
    do
    {
        if (_530 == 0)
        {
            _1147 = (_1131.xyz + _1134.xyz) - (_1131.xyz * _1134.xyz);
            break;
        }
        if (_530 == 1)
        {
            float3 _789 = _1131.xyz * 2.0;
            float3 _794 = _789 - float3(1.0);
            _1147 = mix(_1134.xyz * _789, (_1134.xyz + _794) - (_1134.xyz * _794), fast::max(sign(_1131.xyz - float3(0.5)), float3(0.0)));
            break;
        }
        if (_530 == 2)
        {
            _1147 = fast::min(_1131.xyz, _1134.xyz);
            break;
        }
        if (_530 == 3)
        {
            _1147 = fast::max(_1131.xyz, _1134.xyz);
            break;
        }
        if (_530 == 4)
        {
            float3 _855 = fast::min(float3(1.0), _1131.xyz / (float3(1.0) - _1134.xyz));
            float3 _1141;
            if (_1131.x < 0.0009765625)
            {
                float3 _1087 = _855;
                _1087.x = 0.0;
                _1141 = _1087;
            }
            else
            {
                _1141 = _855;
            }
            float3 _1142;
            if (_1131.y < 0.0009765625)
            {
                float3 _1090 = _1141;
                _1090.y = 0.0;
                _1142 = _1090;
            }
            else
            {
                _1142 = _1141;
            }
            float3 _1143;
            if (_1131.z < 0.0009765625)
            {
                float3 _1093 = _1142;
                _1093.z = 0.0;
                _1143 = _1093;
            }
            else
            {
                _1143 = _1142;
            }
            float3 _1144;
            if ((1.0 - _1134.x) < 0.0009765625)
            {
                float3 _1096 = _1143;
                _1096.x = 1.0;
                _1144 = _1096;
            }
            else
            {
                _1144 = _1143;
            }
            float3 _1145;
            if ((1.0 - _1134.y) < 0.0009765625)
            {
                float3 _1099 = _1144;
                _1099.y = 1.0;
                _1145 = _1099;
            }
            else
            {
                _1145 = _1144;
            }
            float3 _1146;
            if ((1.0 - _1134.z) < 0.0009765625)
            {
                float3 _1102 = _1145;
                _1102.z = 1.0;
                _1146 = _1102;
            }
            else
            {
                _1146 = _1145;
            }
            _1147 = _1146;
            break;
        }
        if (_530 == 5)
        {
            float3 _906 = float3(1.0) - fast::min(float3(1.0), (float3(1.0) - _1131.xyz) / _1134.xyz);
            float3 _1135;
            if ((1.0 - _1131.x) < 0.0009765625)
            {
                float3 _1105 = _906;
                _1105.x = 1.0;
                _1135 = _1105;
            }
            else
            {
                _1135 = _906;
            }
            float3 _1136;
            if ((1.0 - _1131.y) < 0.0009765625)
            {
                float3 _1108 = _1135;
                _1108.y = 1.0;
                _1136 = _1108;
            }
            else
            {
                _1136 = _1135;
            }
            float3 _1137;
            if ((1.0 - _1131.z) < 0.0009765625)
            {
                float3 _1111 = _1136;
                _1111.z = 1.0;
                _1137 = _1111;
            }
            else
            {
                _1137 = _1136;
            }
            float3 _1138;
            if (_1134.x < 0.0009765625)
            {
                float3 _1114 = _1137;
                _1114.x = 0.0;
                _1138 = _1114;
            }
            else
            {
                _1138 = _1137;
            }
            float3 _1139;
            if (_1134.y < 0.0009765625)
            {
                float3 _1117 = _1138;
                _1117.y = 0.0;
                _1139 = _1117;
            }
            else
            {
                _1139 = _1138;
            }
            float3 _1140;
            if (_1134.z < 0.0009765625)
            {
                float3 _1120 = _1139;
                _1120.z = 0.0;
                _1140 = _1120;
            }
            else
            {
                _1140 = _1139;
            }
            _1147 = _1140;
            break;
        }
        if (_530 == 6)
        {
            float3 _956 = _1134.xyz * 2.0;
            float3 _961 = _956 - float3(1.0);
            _1147 = mix(_1131.xyz * _956, (_1131.xyz + _961) - (_1131.xyz * _961), fast::max(sign(_1134.xyz - float3(0.5)), float3(0.0)));
            break;
        }
        _1147 = float3(0.0);
        break;
    } while(false);
    float _1016 = _1134.w * _1131.w;
    float4 _1028 = float4(float4(_1147, _1016).xyz * _1016, _1016) + (float4(_1134.xyz * _1134.w, _1134.w) * (1.0 - _1131.w));
    out.frag_color = _1028 + (float4(_1131.xyz * _1131.w, _1131.w) * (1.0 - _1028.w));
    out.frag_color *= frag_info.alpha;
    return out;
}

