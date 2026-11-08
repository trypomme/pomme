#pragma clang diagnostic ignored "-Wmissing-prototypes"

#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

// Returns the determinant of a 2x2 matrix.
static inline __attribute__((always_inline))
float spvDet2x2(float a1, float a2, float b1, float b2)
{
    return a1 * b2 - b1 * a2;
}

// Returns the inverse of a matrix, by using the algorithm of calculating the classical
// adjoint and dividing by the determinant. The contents of the matrix are changed.
static inline __attribute__((always_inline))
float3x3 spvInverse3x3(float3x3 m)
{
    float3x3 adj;	// The adjoint matrix (inverse after dividing by determinant)

    // Create the transpose of the cofactors, as the classical adjoint of the matrix.
    adj[0][0] =  spvDet2x2(m[1][1], m[1][2], m[2][1], m[2][2]);
    adj[0][1] = -spvDet2x2(m[0][1], m[0][2], m[2][1], m[2][2]);
    adj[0][2] =  spvDet2x2(m[0][1], m[0][2], m[1][1], m[1][2]);

    adj[1][0] = -spvDet2x2(m[1][0], m[1][2], m[2][0], m[2][2]);
    adj[1][1] =  spvDet2x2(m[0][0], m[0][2], m[2][0], m[2][2]);
    adj[1][2] = -spvDet2x2(m[0][0], m[0][2], m[1][0], m[1][2]);

    adj[2][0] =  spvDet2x2(m[1][0], m[1][1], m[2][0], m[2][1]);
    adj[2][1] = -spvDet2x2(m[0][0], m[0][1], m[2][0], m[2][1]);
    adj[2][2] =  spvDet2x2(m[0][0], m[0][1], m[1][0], m[1][1]);

    // Calculate the determinant as a combination of the cofactors of the first row.
    float det = (adj[0][0] * m[0][0]) + (adj[0][1] * m[1][0]) + (adj[0][2] * m[2][0]);

    // Divide the classical adjoint matrix by the determinant.
    // If determinant is zero, matrix is not invertable, so leave it unchanged.
    return (det != 0.0f) ? (adj * (1.0f / det)) : m;
}

constant float kind_tmp [[function_constant(0)]];
constant float kind = is_function_constant_defined(kind_tmp) ? kind_tmp : 3.0;

struct FragInfo
{
    float2 center;
    float radius;
    float tile_mode;
    float4 decal_border_color;
    float alpha;
    int colors_length;
    float2 focus;
    float focus_radius;
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

struct imp_conical_gradient_ssbo_fill_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_conical_gradient_ssbo_fill_fragment_main_in
{
    float2 v_position [[user(locn0)]];
};

fragment imp_conical_gradient_ssbo_fill_fragment_main_out imp_conical_gradient_ssbo_fill_fragment_main(imp_conical_gradient_ssbo_fill_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], const device ColorData& color_data [[buffer(1)]], float4 gl_FragCoord [[position]])
{
    imp_conical_gradient_ssbo_fill_fragment_main_out out = {};
    float2 _1331;
    do
    {
        if (kind == 0.0)
        {
            _1331 = float2(0.0, -1.0);
            break;
        }
        else
        {
            if (kind == 1.0)
            {
                float _821 = frag_info.radius - frag_info.focus_radius;
                _1331 = float2((length((in.v_position - frag_info.focus) * (1.0 / _821)) * sign(_821)) - (frag_info.focus_radius / _821), 1.0);
                break;
            }
            else
            {
                if (kind == 2.0)
                {
                    float2 _1330;
                    do
                    {
                        float _901 = frag_info.center.y - frag_info.focus.y;
                        float _863 = frag_info.focus_radius / distance(frag_info.focus, frag_info.center);
                        float3 _872 = (float3x3(float3(0.0, -1.0, 0.0), float3(1.0, 0.0, 0.0), float3(0.0, 0.0, 1.0)) * spvInverse3x3(float3x3(float3(_901, frag_info.focus.x - frag_info.center.x, 0.0), float3(frag_info.center.x - frag_info.focus.x, _901, 0.0), float3(frag_info.focus, 1.0)))) * float3(in.v_position, 1.0);
                        float _876 = _872.y;
                        float _880 = (_863 * _863) - (_876 * _876);
                        if (_880 < 0.0)
                        {
                            _1330 = float2(0.0, -1.0);
                            break;
                        }
                        _1330 = float2(_872.x + sqrt(_880), 1.0);
                        break;
                    } while(false);
                    _1331 = _1330;
                    break;
                }
                else
                {
                    float2 _1329;
                    do
                    {
                        float _962 = frag_info.focus_radius / (frag_info.focus_radius - frag_info.radius);
                        bool _966 = abs(_962 - 1.0) < 0.000244140625;
                        bool2 _1355 = bool2(_966);
                        float2 _1356 = select(frag_info.center, frag_info.focus, _1355);
                        float _1357 = _966 ? 0.0 : _962;
                        float _975 = 1.0 - _1357;
                        float2 _980 = (select(frag_info.focus, frag_info.center, _1355) * _975) + (_1356 * _1357);
                        float _1133 = _1356.y - _980.y;
                        float _1135 = _980.x;
                        float _1137 = _1356.x;
                        float _986 = abs(_975);
                        float _993 = abs(frag_info.radius - frag_info.focus_radius) / distance(frag_info.focus, frag_info.center);
                        bool _997 = abs(_993 - 1.0) < 0.000244140625;
                        float _1316;
                        float _1317;
                        if (_997)
                        {
                            float _1001 = _986 * 0.5;
                            _1317 = _1001;
                            _1316 = _1001;
                        }
                        else
                        {
                            float _1009 = (_993 * _993) - 1.0;
                            _1317 = _986 / sqrt(abs(_1009));
                            _1316 = _986 * (_993 / _1009);
                        }
                        float3 _1035 = (float3x3(float3(_1316, 0.0, 0.0), float3(0.0, _1317, 0.0), float3(0.0, 0.0, 1.0)) * (float3x3(float3(0.0, -1.0, 0.0), float3(1.0, 0.0, 0.0), float3(0.0, 0.0, 1.0)) * spvInverse3x3(float3x3(float3(_1133, _1135 - _1137, 0.0), float3(_1137 - _1135, _1133, 0.0), float3(_1135, _980.y, 1.0))))) * float3(in.v_position, 1.0);
                        float2 _1036 = _1035.xy;
                        float _1038 = 1.0 / _993;
                        float _1041 = sign(_975);
                        bool _1046 = (!_997) && (_993 > 1.0);
                        float _1320;
                        if (_997)
                        {
                            _1320 = dot(_1036, _1036) / _1035.x;
                        }
                        else
                        {
                            float _1321;
                            if (_1046)
                            {
                                _1321 = length(_1036) - (_1035.x * _1038);
                            }
                            else
                            {
                                float _1067 = _1035.x;
                                float _1072 = _1035.y;
                                float _1076 = (_1067 * _1067) - (_1072 * _1072);
                                float _1322;
                                if (_1076 >= 0.0)
                                {
                                    float _1323;
                                    if (_966 || (_1041 < 0.0))
                                    {
                                        _1323 = (-sqrt(_1076)) - (_1067 * _1038);
                                    }
                                    else
                                    {
                                        _1323 = sqrt(_1076) - (_1067 * _1038);
                                    }
                                    _1322 = _1323;
                                }
                                else
                                {
                                    _1322 = -1.0;
                                }
                                _1321 = _1322;
                            }
                            _1320 = _1321;
                        }
                        if ((!_1046) && (_1320 < 0.0))
                        {
                            _1329 = float2(0.0, -1.0);
                            break;
                        }
                        float _1116 = _1357 + (_1041 * _1320);
                        float _1328;
                        if (_966)
                        {
                            _1328 = 1.0 - _1116;
                        }
                        else
                        {
                            _1328 = _1116;
                        }
                        _1329 = float2(_1328, 1.0);
                        break;
                    } while(false);
                    _1331 = _1329;
                    break;
                }
                break; // unreachable workaround
            }
            break; // unreachable workaround
        }
        break; // unreachable workaround
    } while(false);
    bool _605 = _1331.y < 0.0;
    bool _621;
    if (!_605)
    {
        bool _613 = (_1331.x < 0.0) || (_1331.x > 1.0);
        bool _620;
        if (_613)
        {
            _620 = frag_info.tile_mode == 3.0;
        }
        else
        {
            _620 = _613;
        }
        _621 = _620;
    }
    else
    {
        _621 = _605;
    }
    float4 _1341;
    if (_621)
    {
        _1341 = frag_info.decal_border_color;
    }
    else
    {
        float _1334;
        if (frag_info.tile_mode == 0.0)
        {
            _1334 = fast::clamp(_1331.x, 0.0, 1.0);
        }
        else
        {
            float _1335;
            if (frag_info.tile_mode == 1.0)
            {
                _1335 = fract(_1331.x);
            }
            else
            {
                float _1336;
                if (frag_info.tile_mode == 2.0)
                {
                    float _1183 = _1331.x - 1.0;
                    _1336 = abs((_1183 - (2.0 * floor(_1183 * 0.5))) - 1.0);
                }
                else
                {
                    _1336 = _1331.x;
                }
                _1335 = _1336;
            }
            _1334 = _1335;
        }
        float4 _1342;
        int _1337 = 1;
        for (;;)
        {
            if (_1337 < frag_info.colors_length)
            {
                int _656 = _1337 - 1;
                bool _675 = _1334 >= color_data.colors[_656].stop;
                bool _682;
                if (_675)
                {
                    _682 = _1334 <= color_data.colors[_1337].stop;
                }
                else
                {
                    _682 = _675;
                }
                if (_682)
                {
                    float _690 = color_data.colors[_1337].stop - color_data.colors[_656].stop;
                    float4 _1350;
                    if (_690 < 0.001000000047497451305389404296875)
                    {
                        _1350 = color_data.colors[_1337].color;
                    }
                    else
                    {
                        _1350 = mix(color_data.colors[_656].color, color_data.colors[_1337].color, float4((_1334 - color_data.colors[_656].stop) / _690));
                    }
                    _1342 = _1350;
                    break;
                }
                _1337++;
                continue;
            }
            else
            {
                _1342 = float4(0.0);
                break;
            }
        }
        _1341 = _1342;
    }
    out.frag_color = float4(_1341.xyz * _1341.w, _1341.w) * frag_info.alpha;
    float4 _727 = out.frag_color;
    uint _1219 = uint(gl_FragCoord.x) % 8u;
    uint _1225 = uint(gl_FragCoord.y) ^ _1219;
    float3 _1258 = _727.xyz + float3(((float(((((((_1225 & 1u) << uint(5)) | ((_1219 & 1u) << uint(4))) | ((_1225 & 2u) << uint(2))) | ((_1219 & 2u) << uint(1))) | ((_1225 & 4u) >> uint(1))) | ((_1219 & 4u) >> uint(2))) * 0.015625) - 0.4921875) * 0.015625);
    float4 _1308 = _727;
    _1308.x = _1258.x;
    _1308.y = _1258.y;
    _1308.z = _1258.z;
    out.frag_color = _1308;
    return out;
}

