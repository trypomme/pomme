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
    float stroke_join;
    float aa_pixels;
    float stroked;
    float type;
    float2 superellipse_degree;
    float2 superellipse_semi_axis;
    float2 angle_span;
    float octant_offset_c;
    float2 circle_center_top;
    float2 circle_center_right;
    float2 superellipse_scale;
    float4 radii;
};

struct imp_uber_sdf_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_uber_sdf_fragment_main_in
{
    float2 v_position [[user(locn0)]];
};

fragment imp_uber_sdf_fragment_main_out imp_uber_sdf_fragment_main(imp_uber_sdf_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]])
{
    imp_uber_sdf_fragment_main_out out = {};
    float2 _1011 = in.v_position - frag_info.center;
    float2 _2927;
    if (frag_info.stroked < 0.5)
    {
        float _2913;
        float _2923;
        if (frag_info.type < 0.5)
        {
            float _1187 = length(_1011) - frag_info.size.x;
            _2923 = length(float2(dfdx(_1187), dfdy(_1187)));
            _2913 = _1187;
        }
        else
        {
            float _2914;
            float _2924;
            if (frag_info.type < 1.5)
            {
                float2 _1204 = abs(_1011) - frag_info.size;
                float2 _1224 = dfdx(in.v_position);
                float2 _1226 = dfdy(in.v_position);
                float2 _1245 = abs(_1204);
                _2924 = (_1245.x < _1245.y) ? length(float2(_1224.x, _1226.x)) : length(float2(_1224.y, _1226.y));
                _2914 = length(fast::max(_1204, float2(0.0))) + fast::min(fast::max(_1204.x, _1204.y), 0.0);
            }
            else
            {
                float _2916;
                float _2925;
                if (frag_info.type < 2.5)
                {
                    float2 _1270 = abs(_1011);
                    float2 _1275 = frag_info.size * (_1270 - frag_info.size);
                    float _2910;
                    _2910 = (_1275.x < _1275.y) ? 1.57079637050628662109375 : 0.0;
                    for (int _2909 = 0; _2909 < 5; )
                    {
                        float _1288 = cos(_2910);
                        float _1290 = sin(_2910);
                        float2 _1298 = frag_info.size * float2(_1288, _1290);
                        float2 _1306 = frag_info.size * float2(-_1290, _1288);
                        float2 _1310 = _1270 - _1298;
                        _2910 += (dot(_1310, _1306) / (dot(_1310, _1298) + dot(_1306, _1306)));
                        _2909++;
                        continue;
                    }
                    float _1337 = length(_1270 - (frag_info.size * float2(cos(_2910), sin(_2910))));
                    float2 _1340 = _1270 / frag_info.size;
                    float _2911;
                    if (dot(_1340, _1340) > 1.0)
                    {
                        _2911 = _1337;
                    }
                    else
                    {
                        _2911 = -_1337;
                    }
                    _2925 = length(float2(dfdx(_2911), dfdy(_2911)));
                    _2916 = _2911;
                }
                else
                {
                    float _2917;
                    float _2926;
                    if (frag_info.type < 3.5)
                    {
                        bool _1370 = _1011.x > 0.0;
                        float2 _2900;
                        if (_1370)
                        {
                            _2900 = frag_info.radii.xy;
                        }
                        else
                        {
                            _2900 = frag_info.radii.zw;
                        }
                        bool _1385 = _1011.y > 0.0;
                        float _2946 = _1385 ? _2900.x : _2900.y;
                        float2 _1396 = abs(_1011);
                        float2 _1402 = (_1396 - frag_info.size) + float2(_2946);
                        float2 _1431 = dfdx(in.v_position);
                        float2 _1433 = dfdy(in.v_position);
                        float _1439 = length(float2(_1431.x, _1433.x));
                        float _1445 = length(float2(_1431.y, _1433.y));
                        float2 _2904;
                        if (_1370)
                        {
                            _2904 = frag_info.radii.xy;
                        }
                        else
                        {
                            _2904 = frag_info.radii.zw;
                        }
                        float2 _1483 = _1396 - (frag_info.size - float2(_1385 ? _2904.x : _2904.y));
                        float _1485 = _1483.x;
                        bool _1486 = _1485 > 0.0;
                        bool _1492;
                        if (_1486)
                        {
                            _1492 = _1483.y > 0.0;
                        }
                        else
                        {
                            _1492 = _1486;
                        }
                        float _2908;
                        if (_1492)
                        {
                            _2908 = length(fast::normalize(_1483) * float2(_1439, _1445));
                        }
                        else
                        {
                            _2908 = (_1485 > _1483.y) ? _1439 : _1445;
                        }
                        _2926 = _2908;
                        _2917 = (fast::min(fast::max(_1402.x, _1402.y), 0.0) + length(fast::max(_1402, float2(0.0)))) - _2946;
                    }
                    else
                    {
                        float _2899;
                        do
                        {
                            float2 _1538 = abs(_1011) / frag_info.superellipse_scale;
                            bool _1545 = (_1538.y + frag_info.octant_offset_c) > _1538.x;
                            float2 _2883;
                            float _2885;
                            float _2886;
                            float _2887;
                            float _2898;
                            if (_1545)
                            {
                                _2898 = frag_info.radii.x;
                                _2887 = frag_info.superellipse_degree.x;
                                _2886 = frag_info.superellipse_semi_axis.x;
                                _2885 = frag_info.angle_span.x;
                                _2883 = _1538 + float2(0.0, frag_info.octant_offset_c);
                            }
                            else
                            {
                                _2898 = frag_info.radii.y;
                                _2887 = frag_info.superellipse_degree.y;
                                _2886 = frag_info.superellipse_semi_axis.y;
                                _2885 = frag_info.angle_span.y;
                                _2883 = _1538.yx - float2(0.0, frag_info.octant_offset_c);
                            }
                            float2 _1578 = _2883 - select(frag_info.circle_center_right, frag_info.circle_center_top, bool2(_1545));
                            if (abs(mod(precise::atan2(_1578.y, _1578.x) + 2.35619449615478515625, 6.283185482025146484375) - 3.1415927410125732421875) < abs(_2885))
                            {
                                _2899 = length(_1578) - _2898;
                                break;
                            }
                            float2 _1633 = abs(_2883 / float2(_2886));
                            float2 _2894;
                            if (_1633.y > _1633.x)
                            {
                                _2894 = _1633.yx;
                            }
                            else
                            {
                                _2894 = _1633;
                            }
                            float _1644 = 2.0 / _2887;
                            float _2890;
                            float _2892;
                            _2892 = 0.785398185253143310546875;
                            _2890 = 0.0;
                            for (int _2889 = 0; _2889 < 6; )
                            {
                                float _1652 = _2890 + _2892;
                                float _1653 = 0.5 * _1652;
                                float _1655 = cos(_1653);
                                float _1657 = sin(_1653);
                                float _1660 = powr(_1655, _1644);
                                float _1663 = powr(_1657, _1644);
                                bool _1686 = (((((_2894.x - _1660) * _1660) * _1657) * _1657) - ((((_2894.y - _1663) * _1663) * _1655) * _1655)) < 0.0;
                                _2892 = _1686 ? _2892 : _1653;
                                _2890 = _1686 ? _1653 : _2890;
                                _2889++;
                                continue;
                            }
                            float2 _1702 = float2(_1644);
                            float2 _1703 = powr(float2(cos(_2890), sin(_2890)), _1702);
                            float2 _1714 = _2894 - _1703;
                            float2 _1717 = powr(float2(cos(_2892), sin(_2892)), _1702) - _1703;
                            _2899 = (length(_1714 - (_1717 * fast::clamp(dot(_1714, _1717) / dot(_1717, _1717), 0.0, 1.0))) * sign((_1714.x * _1717.y) - (_1714.y * _1717.x))) * _2886;
                            break;
                        } while(false);
                        _2926 = length(float2(dfdx(_2899), dfdy(_2899)));
                        _2917 = _2899;
                    }
                    _2925 = _2926;
                    _2916 = _2917;
                }
                _2924 = _2925;
                _2914 = _2916;
            }
            _2923 = _2924;
            _2913 = _2914;
        }
        _2927 = float2(_2913, _2923);
    }
    else
    {
        float _2865;
        float _2875;
        if (frag_info.type < 0.5)
        {
            float _1983 = length(_1011) - frag_info.size.x;
            _2875 = length(float2(dfdx(_1983), dfdy(_1983)));
            _2865 = _1983;
        }
        else
        {
            float _2866;
            float _2876;
            if (frag_info.type < 1.5)
            {
                float2 _2000 = abs(_1011) - frag_info.size;
                float2 _2020 = dfdx(in.v_position);
                float2 _2022 = dfdy(in.v_position);
                float2 _2041 = abs(_2000);
                _2876 = (_2041.x < _2041.y) ? length(float2(_2020.x, _2022.x)) : length(float2(_2020.y, _2022.y));
                _2866 = length(fast::max(_2000, float2(0.0))) + fast::min(fast::max(_2000.x, _2000.y), 0.0);
            }
            else
            {
                float _2868;
                float _2877;
                if (frag_info.type < 2.5)
                {
                    float2 _2066 = abs(_1011);
                    float2 _2071 = frag_info.size * (_2066 - frag_info.size);
                    float _2862;
                    _2862 = (_2071.x < _2071.y) ? 1.57079637050628662109375 : 0.0;
                    for (int _2861 = 0; _2861 < 5; )
                    {
                        float _2084 = cos(_2862);
                        float _2086 = sin(_2862);
                        float2 _2094 = frag_info.size * float2(_2084, _2086);
                        float2 _2102 = frag_info.size * float2(-_2086, _2084);
                        float2 _2106 = _2066 - _2094;
                        _2862 += (dot(_2106, _2102) / (dot(_2106, _2094) + dot(_2102, _2102)));
                        _2861++;
                        continue;
                    }
                    float _2133 = length(_2066 - (frag_info.size * float2(cos(_2862), sin(_2862))));
                    float2 _2136 = _2066 / frag_info.size;
                    float _2863;
                    if (dot(_2136, _2136) > 1.0)
                    {
                        _2863 = _2133;
                    }
                    else
                    {
                        _2863 = -_2133;
                    }
                    _2877 = length(float2(dfdx(_2863), dfdy(_2863)));
                    _2868 = _2863;
                }
                else
                {
                    float _2869;
                    float _2878;
                    if (frag_info.type < 3.5)
                    {
                        bool _2166 = _1011.x > 0.0;
                        float2 _2852;
                        if (_2166)
                        {
                            _2852 = frag_info.radii.xy;
                        }
                        else
                        {
                            _2852 = frag_info.radii.zw;
                        }
                        bool _2181 = _1011.y > 0.0;
                        float _2955 = _2181 ? _2852.x : _2852.y;
                        float2 _2192 = abs(_1011);
                        float2 _2198 = (_2192 - frag_info.size) + float2(_2955);
                        float2 _2227 = dfdx(in.v_position);
                        float2 _2229 = dfdy(in.v_position);
                        float _2235 = length(float2(_2227.x, _2229.x));
                        float _2241 = length(float2(_2227.y, _2229.y));
                        float2 _2856;
                        if (_2166)
                        {
                            _2856 = frag_info.radii.xy;
                        }
                        else
                        {
                            _2856 = frag_info.radii.zw;
                        }
                        float2 _2279 = _2192 - (frag_info.size - float2(_2181 ? _2856.x : _2856.y));
                        float _2281 = _2279.x;
                        bool _2282 = _2281 > 0.0;
                        bool _2288;
                        if (_2282)
                        {
                            _2288 = _2279.y > 0.0;
                        }
                        else
                        {
                            _2288 = _2282;
                        }
                        float _2860;
                        if (_2288)
                        {
                            _2860 = length(fast::normalize(_2279) * float2(_2235, _2241));
                        }
                        else
                        {
                            _2860 = (_2281 > _2279.y) ? _2235 : _2241;
                        }
                        _2878 = _2860;
                        _2869 = (fast::min(fast::max(_2198.x, _2198.y), 0.0) + length(fast::max(_2198, float2(0.0)))) - _2955;
                    }
                    else
                    {
                        float _2851;
                        do
                        {
                            float2 _2334 = abs(_1011) / frag_info.superellipse_scale;
                            bool _2341 = (_2334.y + frag_info.octant_offset_c) > _2334.x;
                            float2 _2835;
                            float _2837;
                            float _2838;
                            float _2839;
                            float _2850;
                            if (_2341)
                            {
                                _2850 = frag_info.radii.x;
                                _2839 = frag_info.superellipse_degree.x;
                                _2838 = frag_info.superellipse_semi_axis.x;
                                _2837 = frag_info.angle_span.x;
                                _2835 = _2334 + float2(0.0, frag_info.octant_offset_c);
                            }
                            else
                            {
                                _2850 = frag_info.radii.y;
                                _2839 = frag_info.superellipse_degree.y;
                                _2838 = frag_info.superellipse_semi_axis.y;
                                _2837 = frag_info.angle_span.y;
                                _2835 = _2334.yx - float2(0.0, frag_info.octant_offset_c);
                            }
                            float2 _2374 = _2835 - select(frag_info.circle_center_right, frag_info.circle_center_top, bool2(_2341));
                            if (abs(mod(precise::atan2(_2374.y, _2374.x) + 2.35619449615478515625, 6.283185482025146484375) - 3.1415927410125732421875) < abs(_2837))
                            {
                                _2851 = length(_2374) - _2850;
                                break;
                            }
                            float2 _2429 = abs(_2835 / float2(_2838));
                            float2 _2846;
                            if (_2429.y > _2429.x)
                            {
                                _2846 = _2429.yx;
                            }
                            else
                            {
                                _2846 = _2429;
                            }
                            float _2440 = 2.0 / _2839;
                            float _2842;
                            float _2844;
                            _2844 = 0.785398185253143310546875;
                            _2842 = 0.0;
                            for (int _2841 = 0; _2841 < 6; )
                            {
                                float _2448 = _2842 + _2844;
                                float _2449 = 0.5 * _2448;
                                float _2451 = cos(_2449);
                                float _2453 = sin(_2449);
                                float _2456 = powr(_2451, _2440);
                                float _2459 = powr(_2453, _2440);
                                bool _2482 = (((((_2846.x - _2456) * _2456) * _2453) * _2453) - ((((_2846.y - _2459) * _2459) * _2451) * _2451)) < 0.0;
                                _2844 = _2482 ? _2844 : _2449;
                                _2842 = _2482 ? _2449 : _2842;
                                _2841++;
                                continue;
                            }
                            float2 _2498 = float2(_2440);
                            float2 _2499 = powr(float2(cos(_2842), sin(_2842)), _2498);
                            float2 _2510 = _2846 - _2499;
                            float2 _2513 = powr(float2(cos(_2844), sin(_2844)), _2498) - _2499;
                            _2851 = (length(_2510 - (_2513 * fast::clamp(dot(_2510, _2513) / dot(_2513, _2513), 0.0, 1.0))) * sign((_2510.x * _2513.y) - (_2510.y * _2513.x))) * _2838;
                            break;
                        } while(false);
                        _2878 = length(float2(dfdx(_2851), dfdy(_2851)));
                        _2869 = _2851;
                    }
                    _2877 = _2878;
                    _2868 = _2869;
                }
                _2876 = _2877;
                _2866 = _2868;
            }
            _2875 = _2876;
            _2865 = _2866;
        }
        float _1789 = fast::max(frag_info.stroke_width, _2875) * 0.5;
        bool _1792 = frag_info.type >= 0.5;
        bool _1798;
        if (_1792)
        {
            _1798 = frag_info.type < 1.5;
        }
        else
        {
            _1798 = _1792;
        }
        bool _1804;
        if (_1798)
        {
            _1804 = frag_info.stroke_join < 0.5;
        }
        else
        {
            _1804 = _1798;
        }
        float _2879;
        float _2881;
        if (_1804)
        {
            float2 _2557 = abs(_1011) - (frag_info.size + float2(_1789));
            float _1819 = fast::max(length(fast::max(_2557, float2(0.0))) + fast::min(fast::max(_2557.x, _2557.y), 0.0), -(_2865 + _1789));
            _2881 = length(float2(dfdx(_1819), dfdy(_1819)));
            _2879 = _1819;
        }
        else
        {
            bool _1831;
            if (_1792)
            {
                _1831 = frag_info.type < 1.5;
            }
            else
            {
                _1831 = _1792;
            }
            bool _1837;
            if (_1831)
            {
                _1837 = frag_info.stroke_join >= 0.5;
            }
            else
            {
                _1837 = _1831;
            }
            bool _1843;
            if (_1837)
            {
                _1843 = frag_info.stroke_join < 1.5;
            }
            else
            {
                _1843 = _1837;
            }
            float _2880;
            float _2882;
            if (_1843)
            {
                float2 _1849 = frag_info.size + float2(_1789);
                float2 _2584 = abs(_1011);
                float _2586 = _2584.x;
                float _2588 = _1849.x;
                float _2591 = _2584.y;
                float _2593 = _1849.y;
                float _1859 = fast::max(fast::max(fast::max(_2586 - _2588, _2591 - _2593), ((((_2586 + _2591) - _2588) - _2593) + _1789) * 0.707106769084930419921875), -(_2865 + _1789));
                _2882 = length(float2(dfdx(_1859), dfdy(_1859)));
                _2880 = _1859;
            }
            else
            {
                _2882 = _2875;
                _2880 = abs(_2865) - _1789;
            }
            _2881 = _2882;
            _2879 = _2880;
        }
        _2927 = float2(_2879, _2881);
    }
    float _2643 = _2927.y * frag_info.aa_pixels;
    float _1043 = fast::clamp(1.0 - smoothstep(_2643 * (-0.5), _2643 * 0.5, _2927.x), 0.0, 1.0);
    out.frag_color = float4(frag_info.color.xyz, frag_info.color.w * mix(1.0 - sqrt(1.0 - _1043), sqrt(_1043), dot(frag_info.color.xyz, float3(0.2125999927520751953125, 0.715200006961822509765625, 0.072200000286102294921875))));
    out.frag_color = float4(out.frag_color.xyz * out.frag_color.w, out.frag_color.w);
    return out;
}

