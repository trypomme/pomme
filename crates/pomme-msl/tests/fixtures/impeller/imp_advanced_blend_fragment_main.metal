#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

constant float supports_decal_tmp [[function_constant(1)]];
constant float supports_decal = is_function_constant_defined(supports_decal_tmp) ? supports_decal_tmp : 1.0;

struct BlendInfo
{
    float dst_input_alpha;
    float src_input_alpha;
    float color_factor;
    float4 color;
    float supports_decal_sampler_address_mode;
};

constant float blend_type_tmp [[function_constant(0)]];
constant float blend_type = is_function_constant_defined(blend_type_tmp) ? blend_type_tmp : 0.0;

struct imp_advanced_blend_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_advanced_blend_fragment_main_in
{
    float2 v_dst_texture_coords [[user(locn0)]];
    float2 v_src_texture_coords [[user(locn1)]];
};

fragment imp_advanced_blend_fragment_main_out imp_advanced_blend_fragment_main(imp_advanced_blend_fragment_main_in in [[stage_in]], constant BlendInfo& blend_info [[buffer(0)]], texture2d<float> texture_sampler_dst [[texture(0)]], texture2d<float> texture_sampler_src [[texture(1)]], sampler texture_sampler_dstSmplr [[sampler(0)]], sampler texture_sampler_srcSmplr [[sampler(1)]])
{
    imp_advanced_blend_fragment_main_out out = {};
    bool _986;
    float4 _2449;
    do
    {
        _986 = supports_decal > 0.0;
        if (_986)
        {
            _2449 = texture_sampler_dst.sample(texture_sampler_dstSmplr, in.v_dst_texture_coords);
            break;
        }
        float4 _2448;
        do
        {
            bool _1003 = any(in.v_dst_texture_coords < float2(0.0));
            bool _1010;
            if (!_1003)
            {
                _1010 = any(in.v_dst_texture_coords >= float2(1.0));
            }
            else
            {
                _1010 = _1003;
            }
            if (_1010)
            {
                _2448 = float4(0.0);
                break;
            }
            _2448 = texture_sampler_dst.sample(texture_sampler_dstSmplr, in.v_dst_texture_coords, bias(-0.4749999940395355224609375));
            break;
        } while(false);
        _2449 = _2448;
        break;
    } while(false);
    float4 _2450;
    do
    {
        if (_2449.w == 0.0)
        {
            _2450 = float4(0.0);
            break;
        }
        _2450 = float4(_2449.xyz / float3(_2449.w), _2449.w);
        break;
    } while(false);
    float _899 = _2450.w * blend_info.dst_input_alpha;
    float4 _2454;
    if (blend_info.color_factor > 0.0)
    {
        _2454 = blend_info.color;
    }
    else
    {
        float4 _2452;
        do
        {
            if (_986)
            {
                _2452 = texture_sampler_src.sample(texture_sampler_srcSmplr, in.v_src_texture_coords);
                break;
            }
            float4 _2451;
            do
            {
                bool _1065 = any(in.v_src_texture_coords < float2(0.0));
                bool _1072;
                if (!_1065)
                {
                    _1072 = any(in.v_src_texture_coords >= float2(1.0));
                }
                else
                {
                    _1072 = _1065;
                }
                if (_1072)
                {
                    _2451 = float4(0.0);
                    break;
                }
                _2451 = texture_sampler_src.sample(texture_sampler_srcSmplr, in.v_src_texture_coords, bias(-0.4749999940395355224609375));
                break;
            } while(false);
            _2452 = _2451;
            break;
        } while(false);
        float4 _2453;
        do
        {
            if (_2452.w == 0.0)
            {
                _2453 = float4(0.0);
                break;
            }
            _2453 = float4(_2452.xyz / float3(_2452.w), _2452.w);
            break;
        } while(false);
        _2454 = _2453;
    }
    float4 _2455;
    if (blend_info.color_factor == 0.0)
    {
        float4 _2317 = _2454;
        _2317.w = _2454.w * blend_info.src_input_alpha;
        _2455 = _2317;
    }
    else
    {
        _2455 = _2454;
    }
    int _933 = int(blend_type);
    float3 _2478;
    do
    {
        if (_933 == 0)
        {
            _2478 = (_2450.xyz + _2455.xyz) - (_2450.xyz * _2455.xyz);
            break;
        }
        if (_933 == 1)
        {
            float3 _1271 = _2450.xyz * 2.0;
            float3 _1276 = _1271 - float3(1.0);
            _2478 = mix(_2455.xyz * _1271, (_2455.xyz + _1276) - (_2455.xyz * _1276), fast::max(sign(_2450.xyz - float3(0.5)), float3(0.0)));
            break;
        }
        if (_933 == 2)
        {
            _2478 = fast::min(_2450.xyz, _2455.xyz);
            break;
        }
        if (_933 == 3)
        {
            _2478 = fast::max(_2450.xyz, _2455.xyz);
            break;
        }
        if (_933 == 4)
        {
            float3 _1337 = fast::min(float3(1.0), _2450.xyz / (float3(1.0) - _2455.xyz));
            float3 _2472;
            if (_2450.x < 0.0009765625)
            {
                float3 _2320 = _1337;
                _2320.x = 0.0;
                _2472 = _2320;
            }
            else
            {
                _2472 = _1337;
            }
            float3 _2473;
            if (_2450.y < 0.0009765625)
            {
                float3 _2323 = _2472;
                _2323.y = 0.0;
                _2473 = _2323;
            }
            else
            {
                _2473 = _2472;
            }
            float3 _2474;
            if (_2450.z < 0.0009765625)
            {
                float3 _2326 = _2473;
                _2326.z = 0.0;
                _2474 = _2326;
            }
            else
            {
                _2474 = _2473;
            }
            float3 _2475;
            if ((1.0 - _2455.x) < 0.0009765625)
            {
                float3 _2329 = _2474;
                _2329.x = 1.0;
                _2475 = _2329;
            }
            else
            {
                _2475 = _2474;
            }
            float3 _2476;
            if ((1.0 - _2455.y) < 0.0009765625)
            {
                float3 _2332 = _2475;
                _2332.y = 1.0;
                _2476 = _2332;
            }
            else
            {
                _2476 = _2475;
            }
            float3 _2477;
            if ((1.0 - _2455.z) < 0.0009765625)
            {
                float3 _2335 = _2476;
                _2335.z = 1.0;
                _2477 = _2335;
            }
            else
            {
                _2477 = _2476;
            }
            _2478 = _2477;
            break;
        }
        if (_933 == 5)
        {
            float3 _1388 = float3(1.0) - fast::min(float3(1.0), (float3(1.0) - _2450.xyz) / _2455.xyz);
            float3 _2466;
            if ((1.0 - _2450.x) < 0.0009765625)
            {
                float3 _2338 = _1388;
                _2338.x = 1.0;
                _2466 = _2338;
            }
            else
            {
                _2466 = _1388;
            }
            float3 _2467;
            if ((1.0 - _2450.y) < 0.0009765625)
            {
                float3 _2341 = _2466;
                _2341.y = 1.0;
                _2467 = _2341;
            }
            else
            {
                _2467 = _2466;
            }
            float3 _2468;
            if ((1.0 - _2450.z) < 0.0009765625)
            {
                float3 _2344 = _2467;
                _2344.z = 1.0;
                _2468 = _2344;
            }
            else
            {
                _2468 = _2467;
            }
            float3 _2469;
            if (_2455.x < 0.0009765625)
            {
                float3 _2347 = _2468;
                _2347.x = 0.0;
                _2469 = _2347;
            }
            else
            {
                _2469 = _2468;
            }
            float3 _2470;
            if (_2455.y < 0.0009765625)
            {
                float3 _2350 = _2469;
                _2350.y = 0.0;
                _2470 = _2350;
            }
            else
            {
                _2470 = _2469;
            }
            float3 _2471;
            if (_2455.z < 0.0009765625)
            {
                float3 _2353 = _2470;
                _2353.z = 0.0;
                _2471 = _2353;
            }
            else
            {
                _2471 = _2470;
            }
            _2478 = _2471;
            break;
        }
        if (_933 == 6)
        {
            float3 _1438 = _2455.xyz * 2.0;
            float3 _1443 = _1438 - float3(1.0);
            _2478 = mix(_2450.xyz * _1438, (_2450.xyz + _1443) - (_2450.xyz * _1443), fast::max(sign(_2455.xyz - float3(0.5)), float3(0.0)));
            break;
        }
        if (_933 == 7)
        {
            float3 _1512 = _2455.xyz * 2.0;
            _2478 = mix(_2450.xyz - (((float3(1.0) - _1512) * _2450.xyz) * (float3(1.0) - _2450.xyz)), _2450.xyz + ((_1512 - float3(1.0)) * (mix(((((_2450.xyz * 16.0) - float3(12.0)) * _2450.xyz) + float3(4.0)) * _2450.xyz, sqrt(_2450.xyz), fast::max(sign(_2450.xyz - float3(0.25)), float3(0.0))) - _2450.xyz)), fast::max(sign(_2455.xyz - float3(0.5)), float3(0.0)));
            break;
        }
        if (_933 == 8)
        {
            _2478 = abs(_2450.xyz - _2455.xyz);
            break;
        }
        if (_933 == 9)
        {
            _2478 = (_2450.xyz + _2455.xyz) - ((_2450.xyz * 2.0) * _2455.xyz);
            break;
        }
        if (_933 == 10)
        {
            _2478 = _2450.xyz * _2455.xyz;
            break;
        }
        if (_933 == 11)
        {
            float _1649 = fast::min(fast::min(_2455.x, _2455.y), _2455.z);
            float _1657 = fast::max(fast::max(_2455.x, _2455.y), _2455.z);
            float3 _2463;
            if (_1649 < _1657)
            {
                _2463 = ((_2455.xyz - float3(_1649)) * (fast::max(fast::max(_2450.x, _2450.y), _2450.z) - fast::min(fast::min(_2450.x, _2450.y), _2450.z))) / float3(_1657 - _1649);
            }
            else
            {
                _2463 = float3(0.0);
            }
            float3 _1701 = _2463 + float3((((_2450.x * 0.300000011920928955078125) + (_2450.y * 0.589999973773956298828125)) + (_2450.z * 0.10999999940395355224609375)) - (((_2463.x * 0.300000011920928955078125) + (_2463.y * 0.589999973773956298828125)) + (_2463.z * 0.10999999940395355224609375)));
            float _1783 = _1701.x;
            float _1786 = _1701.y;
            float _1790 = _1701.z;
            float _1792 = ((_1783 * 0.300000011920928955078125) + (_1786 * 0.589999973773956298828125)) + (_1790 * 0.10999999940395355224609375);
            float _1731 = fast::min(fast::min(_1783, _1786), _1790);
            float _1739 = fast::max(fast::max(_1783, _1786), _1790);
            float3 _2464;
            if (_1731 < 0.0)
            {
                float3 _1746 = float3(_1792);
                _2464 = _1746 + (((_1701 - _1746) * _1792) / float3((_1792 - _1731) + 0.0009765625));
            }
            else
            {
                _2464 = _1701;
            }
            float3 _2465;
            if (_1739 > 1.0)
            {
                float3 _1765 = float3(_1792);
                _2465 = _1765 + (((_2464 - _1765) * (1.0 - _1792)) / float3((_1739 - _1792) + 0.0009765625));
            }
            else
            {
                _2465 = _2464;
            }
            _2478 = _2465;
            break;
        }
        if (_933 == 12)
        {
            float _1839 = fast::min(fast::min(_2450.x, _2450.y), _2450.z);
            float _1847 = fast::max(fast::max(_2450.x, _2450.y), _2450.z);
            float3 _2460;
            if (_1839 < _1847)
            {
                _2460 = ((_2450.xyz - float3(_1839)) * (fast::max(fast::max(_2455.x, _2455.y), _2455.z) - fast::min(fast::min(_2455.x, _2455.y), _2455.z))) / float3(_1847 - _1839);
            }
            else
            {
                _2460 = float3(0.0);
            }
            float3 _1891 = _2460 + float3((((_2450.x * 0.300000011920928955078125) + (_2450.y * 0.589999973773956298828125)) + (_2450.z * 0.10999999940395355224609375)) - (((_2460.x * 0.300000011920928955078125) + (_2460.y * 0.589999973773956298828125)) + (_2460.z * 0.10999999940395355224609375)));
            float _1973 = _1891.x;
            float _1976 = _1891.y;
            float _1980 = _1891.z;
            float _1982 = ((_1973 * 0.300000011920928955078125) + (_1976 * 0.589999973773956298828125)) + (_1980 * 0.10999999940395355224609375);
            float _1921 = fast::min(fast::min(_1973, _1976), _1980);
            float _1929 = fast::max(fast::max(_1973, _1976), _1980);
            float3 _2461;
            if (_1921 < 0.0)
            {
                float3 _1936 = float3(_1982);
                _2461 = _1936 + (((_1891 - _1936) * _1982) / float3((_1982 - _1921) + 0.0009765625));
            }
            else
            {
                _2461 = _1891;
            }
            float3 _2462;
            if (_1929 > 1.0)
            {
                float3 _1955 = float3(_1982);
                _2462 = _1955 + (((_2461 - _1955) * (1.0 - _1982)) / float3((_1929 - _1982) + 0.0009765625));
            }
            else
            {
                _2462 = _2461;
            }
            _2478 = _2462;
            break;
        }
        if (_933 == 13)
        {
            float3 _2017 = _2455.xyz + float3((((_2450.x * 0.300000011920928955078125) + (_2450.y * 0.589999973773956298828125)) + (_2450.z * 0.10999999940395355224609375)) - (((_2455.x * 0.300000011920928955078125) + (_2455.y * 0.589999973773956298828125)) + (_2455.z * 0.10999999940395355224609375)));
            float _2099 = _2017.x;
            float _2102 = _2017.y;
            float _2106 = _2017.z;
            float _2108 = ((_2099 * 0.300000011920928955078125) + (_2102 * 0.589999973773956298828125)) + (_2106 * 0.10999999940395355224609375);
            float _2047 = fast::min(fast::min(_2099, _2102), _2106);
            float _2055 = fast::max(fast::max(_2099, _2102), _2106);
            float3 _2458;
            if (_2047 < 0.0)
            {
                float3 _2062 = float3(_2108);
                _2458 = _2062 + (((_2017 - _2062) * _2108) / float3((_2108 - _2047) + 0.0009765625));
            }
            else
            {
                _2458 = _2017;
            }
            float3 _2459;
            if (_2055 > 1.0)
            {
                float3 _2081 = float3(_2108);
                _2459 = _2081 + (((_2458 - _2081) * (1.0 - _2108)) / float3((_2055 - _2108) + 0.0009765625));
            }
            else
            {
                _2459 = _2458;
            }
            _2478 = _2459;
            break;
        }
        if (_933 == 14)
        {
            float3 _2143 = _2450.xyz + float3((((_2455.x * 0.300000011920928955078125) + (_2455.y * 0.589999973773956298828125)) + (_2455.z * 0.10999999940395355224609375)) - (((_2450.x * 0.300000011920928955078125) + (_2450.y * 0.589999973773956298828125)) + (_2450.z * 0.10999999940395355224609375)));
            float _2225 = _2143.x;
            float _2228 = _2143.y;
            float _2232 = _2143.z;
            float _2234 = ((_2225 * 0.300000011920928955078125) + (_2228 * 0.589999973773956298828125)) + (_2232 * 0.10999999940395355224609375);
            float _2173 = fast::min(fast::min(_2225, _2228), _2232);
            float _2181 = fast::max(fast::max(_2225, _2228), _2232);
            float3 _2456;
            if (_2173 < 0.0)
            {
                float3 _2188 = float3(_2234);
                _2456 = _2188 + (((_2143 - _2188) * _2234) / float3((_2234 - _2173) + 0.0009765625));
            }
            else
            {
                _2456 = _2143;
            }
            float3 _2457;
            if (_2181 > 1.0)
            {
                float3 _2207 = float3(_2234);
                _2457 = _2207 + (((_2456 - _2207) * (1.0 - _2234)) / float3((_2181 - _2234) + 0.0009765625));
            }
            else
            {
                _2457 = _2456;
            }
            _2478 = _2457;
            break;
        }
        _2478 = float3(0.0);
        break;
    } while(false);
    float _2247 = _2455.w * _899;
    float4 _2259 = float4(float4(_2478, _2247).xyz * _2247, _2247) + (float4(_2455.xyz * _2455.w, _2455.w) * (1.0 - _899));
    out.frag_color = _2259 + (float4(_2450.xyz * _899, _899) * (1.0 - _2259.w));
    return out;
}

