#pragma clang diagnostic ignored "-Wmissing-prototypes"

#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

// Implementation of the GLSL mod() function, which is slightly different than Metal fmod()
template<typename Tx, typename Ty>
inline Tx mod(Tx x, Ty y)
{
    return x - y * floor(x / y);
}

struct FragInfo
{
    float4 color;
    float2 center;
    float2 size;
    float stroke_width;
    float stroked;
    float4 superellipse_degrees_top;
    float4 superellipse_degrees_right;
    float4 superellipse_semi_axes_top;
    float4 superellipse_semi_axes_right;
    float4 angle_spans_top;
    float4 angle_spans_right;
    float4 octant_offsets_c;
    float4 radii_width;
    float4 radii_height;
    float4 circle_centers_top_x;
    float4 circle_centers_top_y;
    float4 circle_centers_right_x;
    float4 circle_centers_right_y;
    float4 superellipse_scales_x;
    float4 superellipse_scales_y;
    float4 quadrant_centers_x;
    float4 quadrant_centers_y;
    float4 quadrant_splits;
};

struct imp_complex_rse_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_complex_rse_fragment_main_in
{
    float2 v_position [[user(locn0)]];
};

fragment imp_complex_rse_fragment_main_out imp_complex_rse_fragment_main(imp_complex_rse_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]])
{
    imp_complex_rse_fragment_main_out out = {};
    float2 _757 = in.v_position - frag_info.center;
    float4 param = frag_info.superellipse_degrees_top;
    float4 param_1 = frag_info.superellipse_degrees_right;
    float4 param_2 = frag_info.superellipse_semi_axes_top;
    float4 param_3 = frag_info.superellipse_semi_axes_right;
    float4 param_4 = frag_info.angle_spans_top;
    float4 param_5 = frag_info.angle_spans_right;
    float4 param_6 = frag_info.octant_offsets_c;
    float4 param_7 = frag_info.radii_width;
    float4 param_8 = frag_info.radii_height;
    float4 param_9 = frag_info.circle_centers_top_x;
    float4 param_10 = frag_info.circle_centers_top_y;
    float4 param_11 = frag_info.circle_centers_right_x;
    float4 param_12 = frag_info.circle_centers_right_y;
    float4 param_13 = frag_info.superellipse_scales_x;
    float4 param_14 = frag_info.superellipse_scales_y;
    float4 param_15 = frag_info.quadrant_centers_x;
    float4 param_16 = frag_info.quadrant_centers_y;
    float _971 = _757.y;
    float _976 = _757.x;
    float _978 = (frag_info.quadrant_splits.x * _971) - ((-frag_info.size.y) * _976);
    float _989 = (frag_info.size.x * _971) - (frag_info.quadrant_splits.w * _976);
    float _1000 = (frag_info.quadrant_splits.y * _971) - (frag_info.size.y * _976);
    bool _1013 = _989 < 0.0;
    bool _1025;
    if (!_1013)
    {
        bool _1017 = _989 == 0.0;
        bool _1023;
        if (_1017)
        {
            _1023 = _976 > 0.0;
        }
        else
        {
            _1023 = _1017;
        }
        _1025 = _1023;
    }
    else
    {
        _1025 = _1013;
    }
    bool _1042;
    if (_1025)
    {
        bool _1028 = _978 > 0.0;
        bool _1040;
        if (!_1028)
        {
            bool _1032 = _978 == 0.0;
            bool _1038;
            if (_1032)
            {
                _1038 = _976 > 0.0;
            }
            else
            {
                _1038 = _1032;
            }
            _1040 = _1038;
        }
        else
        {
            _1040 = _1028;
        }
        _1042 = _1040;
    }
    else
    {
        _1042 = _1025;
    }
    int _1595;
    if (_1042)
    {
        _1595 = 1;
    }
    else
    {
        bool _1046 = _1000 < 0.0;
        bool _1058;
        if (!_1046)
        {
            bool _1050 = _1000 == 0.0;
            bool _1056;
            if (_1050)
            {
                _1056 = _976 > 0.0;
            }
            else
            {
                _1056 = _1050;
            }
            _1058 = _1056;
        }
        else
        {
            _1058 = _1046;
        }
        bool _1075;
        if (_1058)
        {
            bool _1061 = _989 > 0.0;
            bool _1073;
            if (!_1061)
            {
                bool _1065 = _989 == 0.0;
                bool _1071;
                if (_1065)
                {
                    _1071 = _976 > 0.0;
                }
                else
                {
                    _1071 = _1065;
                }
                _1073 = _1071;
            }
            else
            {
                _1073 = _1061;
            }
            _1075 = _1073;
        }
        else
        {
            _1075 = _1058;
        }
        int _1596;
        if (_1075)
        {
            _1596 = 0;
        }
        else
        {
            _1596 = ((_1000 >= 0.0) && ((((-frag_info.size.x) * _971) - (frag_info.quadrant_splits.z * _976)) <= 0.0)) ? 2 : 3;
        }
        _1595 = _1596;
    }
    float2 _1135 = float2(param_13[_1595], param_14[_1595]);
    float _1625;
    do
    {
        float2 _1598;
        if (_1595 == 0)
        {
            _1598 = float2(1.0);
        }
        else
        {
            float2 _1599;
            if (_1595 == 1)
            {
                _1599 = float2(1.0, -1.0);
            }
            else
            {
                _1599 = select(float2(-1.0), float2(-1.0, 1.0), bool2(_1595 == 2));
            }
            _1598 = _1599;
        }
        float2 _1209 = (_757 - float2(param_15[_1595], param_16[_1595])) * _1598;
        float2 _1224 = _1209 - float2(param_13[_1595] * param_2[_1595], param_14[_1595] * param_3[_1595]);
        float _1235 = length(fast::max(_1224, float2(0.0))) + fast::min(fast::max(_1224.x, _1224.y), 0.0);
        bool _1238 = _1209.x <= 0.0;
        bool _1245;
        if (!_1238)
        {
            _1245 = _1209.y <= 0.0;
        }
        else
        {
            _1245 = _1238;
        }
        if (_1245)
        {
            _1625 = _1235;
            break;
        }
        float2 _1251 = fast::max(_1209, float2(0.0)) / _1135;
        float _1255 = _1251.y + param_6[_1595];
        float _1257 = _1251.x;
        bool _1258 = _1255 > _1257;
        float2 _1601;
        if (_1258)
        {
            _1601 = _1251 + float2(0.0, param_6[_1595]);
        }
        else
        {
            _1601 = _1251.yx - float2(0.0, param_6[_1595]);
        }
        float _1647 = _1258 ? param[_1595] : param_1[_1595];
        float _1648 = _1258 ? param_2[_1595] : param_3[_1595];
        float2 _1283 = _1601 - select(float2(param_11[_1595], param_12[_1595]), float2(param_9[_1595], param_10[_1595]), bool2(_1258));
        float2 _1621;
        float _1623;
        if (abs(mod(precise::atan2(_1283.y, _1283.x) + 2.35619449615478515625, 6.283185482025146484375) - 3.1415927410125732421875) < abs(_1258 ? param_4[_1595] : param_5[_1595]))
        {
            _1623 = length(_1283) - (_1258 ? param_7[_1595] : param_8[_1595]);
            _1621 = fast::normalize(_1283);
        }
        else
        {
            float2 _1378 = abs(_1601 / float2(_1648));
            float2 _1612;
            if (_1378.y > _1378.x)
            {
                _1612 = _1378.yx;
            }
            else
            {
                _1612 = _1378;
            }
            float _1389 = 2.0 / _1647;
            float _1608;
            float _1610;
            _1610 = 0.785398185253143310546875;
            _1608 = 0.0;
            for (int _1607 = 0; _1607 < 6; )
            {
                float _1397 = _1608 + _1610;
                float _1398 = 0.5 * _1397;
                float _1400 = cos(_1398);
                float _1402 = sin(_1398);
                float _1405 = powr(_1400, _1389);
                float _1408 = powr(_1402, _1389);
                bool _1431 = (((((_1612.x - _1405) * _1405) * _1402) * _1402) - ((((_1612.y - _1408) * _1408) * _1400) * _1400)) < 0.0;
                _1610 = _1431 ? _1610 : _1398;
                _1608 = _1431 ? _1398 : _1608;
                _1607++;
                continue;
            }
            float2 _1447 = float2(_1389);
            float2 _1448 = powr(float2(cos(_1608), sin(_1608)), _1447);
            float2 _1459 = _1612 - _1448;
            float2 _1462 = powr(float2(cos(_1610), sin(_1610)), _1447) - _1448;
            float2 _1316 = fast::max(_1601, float2(0.001000000047497451305389404296875));
            _1623 = (length(_1459 - (_1462 * fast::clamp(dot(_1459, _1462) / dot(_1462, _1462), 0.0, 1.0))) * sign((_1459.x * _1462.y) - (_1459.y * _1462.x))) * _1648;
            _1621 = fast::normalize(powr(_1316 / float2(fast::max(_1316.x, _1316.y)), float2(_1647 - 1.0)));
        }
        float2 _1624;
        if (_1255 <= _1257)
        {
            _1624 = _1621.yx;
        }
        else
        {
            _1624 = _1621;
        }
        _1625 = fast::max(_1235, _1623 / length(_1624 / _1135));
        break;
    } while(false);
    float _1494 = dfdx(_1625);
    float _1496 = dfdy(_1625);
    float _1499 = length(float2(_1494, _1496));
    float2 _1626;
    if (frag_info.stroked < 0.5)
    {
        _1626 = float2(_1625, _1499);
    }
    else
    {
        _1626 = float2(abs(_1625) - (fast::max(frag_info.stroke_width, _1499) * 0.5), _1499);
    }
    out.frag_color = float4(frag_info.color.xyz, frag_info.color.w * (1.0 - smoothstep(_1626.y * (-0.5), _1626.y * 0.5, _1626.x)));
    out.frag_color = float4(out.frag_color.xyz * out.frag_color.w, out.frag_color.w);
    return out;
}

