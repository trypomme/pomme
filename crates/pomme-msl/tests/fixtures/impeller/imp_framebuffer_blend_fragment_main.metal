#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

constant float supports_decal_tmp [[function_constant(1)]];
constant float supports_decal = is_function_constant_defined(supports_decal_tmp) ? supports_decal_tmp : 1.0;

struct FragInfo
{
    float src_input_alpha;
    float dst_input_alpha;
};

constant float blend_type_tmp [[function_constant(0)]];
constant float blend_type = is_function_constant_defined(blend_type_tmp) ? blend_type_tmp : 0.0;

struct imp_framebuffer_blend_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_framebuffer_blend_fragment_main_in
{
    float2 v_src_texture_coords [[user(locn0)]];
};

fragment imp_framebuffer_blend_fragment_main_out imp_framebuffer_blend_fragment_main(imp_framebuffer_blend_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], float4 uSub [[color(0)]], texture2d<float> texture_sampler_src [[texture(0)]], sampler texture_sampler_srcSmplr [[sampler(0)]])
{
    imp_framebuffer_blend_fragment_main_out out = {};
    float4 _1000 = (((uSub + uSub) + uSub) + uSub) * float4(0.25);
    float4 _2435;
    do
    {
        float _1007 = _1000.w;
        if (_1007 == 0.0)
        {
            _2435 = float4(0.0);
            break;
        }
        _2435 = float4(_1000.xyz / float3(_1007), _1007);
        break;
    } while(false);
    float _921 = _2435.w * frag_info.dst_input_alpha;
    float4 _2437;
    do
    {
        if (supports_decal > 1.0)
        {
            _2437 = texture_sampler_src.sample(texture_sampler_srcSmplr, in.v_src_texture_coords);
            break;
        }
        float4 _2436;
        do
        {
            bool _1051 = any(in.v_src_texture_coords < float2(0.0));
            bool _1058;
            if (!_1051)
            {
                _1058 = any(in.v_src_texture_coords >= float2(1.0));
            }
            else
            {
                _1058 = _1051;
            }
            if (_1058)
            {
                _2436 = float4(0.0);
                break;
            }
            _2436 = texture_sampler_src.sample(texture_sampler_srcSmplr, in.v_src_texture_coords, bias(-0.4749999940395355224609375));
            break;
        } while(false);
        _2437 = _2436;
        break;
    } while(false);
    float4 _2438;
    do
    {
        if (_2437.w == 0.0)
        {
            _2438 = float4(0.0);
            break;
        }
        _2438 = float4(_2437.xyz / float3(_2437.w), _2437.w);
        break;
    } while(false);
    float _936 = _2438.w * frag_info.src_input_alpha;
    int _940 = int(blend_type);
    float3 _2461;
    do
    {
        if (_940 == 0)
        {
            _2461 = (_2435.xyz + _2438.xyz) - (_2435.xyz * _2438.xyz);
            break;
        }
        if (_940 == 1)
        {
            float3 _1258 = _2435.xyz * 2.0;
            float3 _1263 = _1258 - float3(1.0);
            _2461 = mix(_2438.xyz * _1258, (_2438.xyz + _1263) - (_2438.xyz * _1263), fast::max(sign(_2435.xyz - float3(0.5)), float3(0.0)));
            break;
        }
        if (_940 == 2)
        {
            _2461 = fast::min(_2435.xyz, _2438.xyz);
            break;
        }
        if (_940 == 3)
        {
            _2461 = fast::max(_2435.xyz, _2438.xyz);
            break;
        }
        if (_940 == 4)
        {
            float3 _1324 = fast::min(float3(1.0), _2435.xyz / (float3(1.0) - _2438.xyz));
            float3 _2455;
            if (_2435.x < 0.0009765625)
            {
                float3 _2307 = _1324;
                _2307.x = 0.0;
                _2455 = _2307;
            }
            else
            {
                _2455 = _1324;
            }
            float3 _2456;
            if (_2435.y < 0.0009765625)
            {
                float3 _2310 = _2455;
                _2310.y = 0.0;
                _2456 = _2310;
            }
            else
            {
                _2456 = _2455;
            }
            float3 _2457;
            if (_2435.z < 0.0009765625)
            {
                float3 _2313 = _2456;
                _2313.z = 0.0;
                _2457 = _2313;
            }
            else
            {
                _2457 = _2456;
            }
            float3 _2458;
            if ((1.0 - _2438.x) < 0.0009765625)
            {
                float3 _2316 = _2457;
                _2316.x = 1.0;
                _2458 = _2316;
            }
            else
            {
                _2458 = _2457;
            }
            float3 _2459;
            if ((1.0 - _2438.y) < 0.0009765625)
            {
                float3 _2319 = _2458;
                _2319.y = 1.0;
                _2459 = _2319;
            }
            else
            {
                _2459 = _2458;
            }
            float3 _2460;
            if ((1.0 - _2438.z) < 0.0009765625)
            {
                float3 _2322 = _2459;
                _2322.z = 1.0;
                _2460 = _2322;
            }
            else
            {
                _2460 = _2459;
            }
            _2461 = _2460;
            break;
        }
        if (_940 == 5)
        {
            float3 _1375 = float3(1.0) - fast::min(float3(1.0), (float3(1.0) - _2435.xyz) / _2438.xyz);
            float3 _2449;
            if ((1.0 - _2435.x) < 0.0009765625)
            {
                float3 _2325 = _1375;
                _2325.x = 1.0;
                _2449 = _2325;
            }
            else
            {
                _2449 = _1375;
            }
            float3 _2450;
            if ((1.0 - _2435.y) < 0.0009765625)
            {
                float3 _2328 = _2449;
                _2328.y = 1.0;
                _2450 = _2328;
            }
            else
            {
                _2450 = _2449;
            }
            float3 _2451;
            if ((1.0 - _2435.z) < 0.0009765625)
            {
                float3 _2331 = _2450;
                _2331.z = 1.0;
                _2451 = _2331;
            }
            else
            {
                _2451 = _2450;
            }
            float3 _2452;
            if (_2438.x < 0.0009765625)
            {
                float3 _2334 = _2451;
                _2334.x = 0.0;
                _2452 = _2334;
            }
            else
            {
                _2452 = _2451;
            }
            float3 _2453;
            if (_2438.y < 0.0009765625)
            {
                float3 _2337 = _2452;
                _2337.y = 0.0;
                _2453 = _2337;
            }
            else
            {
                _2453 = _2452;
            }
            float3 _2454;
            if (_2438.z < 0.0009765625)
            {
                float3 _2340 = _2453;
                _2340.z = 0.0;
                _2454 = _2340;
            }
            else
            {
                _2454 = _2453;
            }
            _2461 = _2454;
            break;
        }
        if (_940 == 6)
        {
            float3 _1425 = _2438.xyz * 2.0;
            float3 _1430 = _1425 - float3(1.0);
            _2461 = mix(_2435.xyz * _1425, (_2435.xyz + _1430) - (_2435.xyz * _1430), fast::max(sign(_2438.xyz - float3(0.5)), float3(0.0)));
            break;
        }
        if (_940 == 7)
        {
            float3 _1499 = _2438.xyz * 2.0;
            _2461 = mix(_2435.xyz - (((float3(1.0) - _1499) * _2435.xyz) * (float3(1.0) - _2435.xyz)), _2435.xyz + ((_1499 - float3(1.0)) * (mix(((((_2435.xyz * 16.0) - float3(12.0)) * _2435.xyz) + float3(4.0)) * _2435.xyz, sqrt(_2435.xyz), fast::max(sign(_2435.xyz - float3(0.25)), float3(0.0))) - _2435.xyz)), fast::max(sign(_2438.xyz - float3(0.5)), float3(0.0)));
            break;
        }
        if (_940 == 8)
        {
            _2461 = abs(_2435.xyz - _2438.xyz);
            break;
        }
        if (_940 == 9)
        {
            _2461 = (_2435.xyz + _2438.xyz) - ((_2435.xyz * 2.0) * _2438.xyz);
            break;
        }
        if (_940 == 10)
        {
            _2461 = _2435.xyz * _2438.xyz;
            break;
        }
        if (_940 == 11)
        {
            float _1636 = fast::min(fast::min(_2438.x, _2438.y), _2438.z);
            float _1644 = fast::max(fast::max(_2438.x, _2438.y), _2438.z);
            float3 _2446;
            if (_1636 < _1644)
            {
                _2446 = ((_2438.xyz - float3(_1636)) * (fast::max(fast::max(_2435.x, _2435.y), _2435.z) - fast::min(fast::min(_2435.x, _2435.y), _2435.z))) / float3(_1644 - _1636);
            }
            else
            {
                _2446 = float3(0.0);
            }
            float3 _1688 = _2446 + float3((((_2435.x * 0.300000011920928955078125) + (_2435.y * 0.589999973773956298828125)) + (_2435.z * 0.10999999940395355224609375)) - (((_2446.x * 0.300000011920928955078125) + (_2446.y * 0.589999973773956298828125)) + (_2446.z * 0.10999999940395355224609375)));
            float _1770 = _1688.x;
            float _1773 = _1688.y;
            float _1777 = _1688.z;
            float _1779 = ((_1770 * 0.300000011920928955078125) + (_1773 * 0.589999973773956298828125)) + (_1777 * 0.10999999940395355224609375);
            float _1718 = fast::min(fast::min(_1770, _1773), _1777);
            float _1726 = fast::max(fast::max(_1770, _1773), _1777);
            float3 _2447;
            if (_1718 < 0.0)
            {
                float3 _1733 = float3(_1779);
                _2447 = _1733 + (((_1688 - _1733) * _1779) / float3((_1779 - _1718) + 0.0009765625));
            }
            else
            {
                _2447 = _1688;
            }
            float3 _2448;
            if (_1726 > 1.0)
            {
                float3 _1752 = float3(_1779);
                _2448 = _1752 + (((_2447 - _1752) * (1.0 - _1779)) / float3((_1726 - _1779) + 0.0009765625));
            }
            else
            {
                _2448 = _2447;
            }
            _2461 = _2448;
            break;
        }
        if (_940 == 12)
        {
            float _1826 = fast::min(fast::min(_2435.x, _2435.y), _2435.z);
            float _1834 = fast::max(fast::max(_2435.x, _2435.y), _2435.z);
            float3 _2443;
            if (_1826 < _1834)
            {
                _2443 = ((_2435.xyz - float3(_1826)) * (fast::max(fast::max(_2438.x, _2438.y), _2438.z) - fast::min(fast::min(_2438.x, _2438.y), _2438.z))) / float3(_1834 - _1826);
            }
            else
            {
                _2443 = float3(0.0);
            }
            float3 _1878 = _2443 + float3((((_2435.x * 0.300000011920928955078125) + (_2435.y * 0.589999973773956298828125)) + (_2435.z * 0.10999999940395355224609375)) - (((_2443.x * 0.300000011920928955078125) + (_2443.y * 0.589999973773956298828125)) + (_2443.z * 0.10999999940395355224609375)));
            float _1960 = _1878.x;
            float _1963 = _1878.y;
            float _1967 = _1878.z;
            float _1969 = ((_1960 * 0.300000011920928955078125) + (_1963 * 0.589999973773956298828125)) + (_1967 * 0.10999999940395355224609375);
            float _1908 = fast::min(fast::min(_1960, _1963), _1967);
            float _1916 = fast::max(fast::max(_1960, _1963), _1967);
            float3 _2444;
            if (_1908 < 0.0)
            {
                float3 _1923 = float3(_1969);
                _2444 = _1923 + (((_1878 - _1923) * _1969) / float3((_1969 - _1908) + 0.0009765625));
            }
            else
            {
                _2444 = _1878;
            }
            float3 _2445;
            if (_1916 > 1.0)
            {
                float3 _1942 = float3(_1969);
                _2445 = _1942 + (((_2444 - _1942) * (1.0 - _1969)) / float3((_1916 - _1969) + 0.0009765625));
            }
            else
            {
                _2445 = _2444;
            }
            _2461 = _2445;
            break;
        }
        if (_940 == 13)
        {
            float3 _2004 = _2438.xyz + float3((((_2435.x * 0.300000011920928955078125) + (_2435.y * 0.589999973773956298828125)) + (_2435.z * 0.10999999940395355224609375)) - (((_2438.x * 0.300000011920928955078125) + (_2438.y * 0.589999973773956298828125)) + (_2438.z * 0.10999999940395355224609375)));
            float _2086 = _2004.x;
            float _2089 = _2004.y;
            float _2093 = _2004.z;
            float _2095 = ((_2086 * 0.300000011920928955078125) + (_2089 * 0.589999973773956298828125)) + (_2093 * 0.10999999940395355224609375);
            float _2034 = fast::min(fast::min(_2086, _2089), _2093);
            float _2042 = fast::max(fast::max(_2086, _2089), _2093);
            float3 _2441;
            if (_2034 < 0.0)
            {
                float3 _2049 = float3(_2095);
                _2441 = _2049 + (((_2004 - _2049) * _2095) / float3((_2095 - _2034) + 0.0009765625));
            }
            else
            {
                _2441 = _2004;
            }
            float3 _2442;
            if (_2042 > 1.0)
            {
                float3 _2068 = float3(_2095);
                _2442 = _2068 + (((_2441 - _2068) * (1.0 - _2095)) / float3((_2042 - _2095) + 0.0009765625));
            }
            else
            {
                _2442 = _2441;
            }
            _2461 = _2442;
            break;
        }
        if (_940 == 14)
        {
            float3 _2130 = _2435.xyz + float3((((_2438.x * 0.300000011920928955078125) + (_2438.y * 0.589999973773956298828125)) + (_2438.z * 0.10999999940395355224609375)) - (((_2435.x * 0.300000011920928955078125) + (_2435.y * 0.589999973773956298828125)) + (_2435.z * 0.10999999940395355224609375)));
            float _2212 = _2130.x;
            float _2215 = _2130.y;
            float _2219 = _2130.z;
            float _2221 = ((_2212 * 0.300000011920928955078125) + (_2215 * 0.589999973773956298828125)) + (_2219 * 0.10999999940395355224609375);
            float _2160 = fast::min(fast::min(_2212, _2215), _2219);
            float _2168 = fast::max(fast::max(_2212, _2215), _2219);
            float3 _2439;
            if (_2160 < 0.0)
            {
                float3 _2175 = float3(_2221);
                _2439 = _2175 + (((_2130 - _2175) * _2221) / float3((_2221 - _2160) + 0.0009765625));
            }
            else
            {
                _2439 = _2130;
            }
            float3 _2440;
            if (_2168 > 1.0)
            {
                float3 _2194 = float3(_2221);
                _2440 = _2194 + (((_2439 - _2194) * (1.0 - _2221)) / float3((_2168 - _2221) + 0.0009765625));
            }
            else
            {
                _2440 = _2439;
            }
            _2461 = _2440;
            break;
        }
        _2461 = float3(0.0);
        break;
    } while(false);
    float _2234 = _936 * _921;
    float4 _2246 = float4(float4(_2461, _2234).xyz * _2234, _2234) + (float4(_2438.xyz * _936, _936) * (1.0 - _921));
    out.frag_color = _2246 + (float4(_2435.xyz * _921, _921) * (1.0 - _2246.w));
    return out;
}

