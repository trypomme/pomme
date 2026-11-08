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

struct imp_vertices_uber_2_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_vertices_uber_2_fragment_main_in
{
    float2 v_texture_coords [[user(locn0)]];
    float4 v_color [[user(locn1)]];
};

fragment imp_vertices_uber_2_fragment_main_out imp_vertices_uber_2_fragment_main(imp_vertices_uber_2_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], texture2d<float> texture_sampler [[texture(0)]], sampler texture_samplerSmplr [[sampler(0)]])
{
    imp_vertices_uber_2_fragment_main_out out = {};
    float4 _1863;
    do
    {
        if (in.v_color.w == 0.0)
        {
            _1863 = float4(0.0);
            break;
        }
        _1863 = float4(in.v_color.xyz / float3(in.v_color.w), in.v_color.w);
        break;
    } while(false);
    float4 _1865;
    do
    {
        if (supports_decal > 0.0)
        {
            _1865 = texture_sampler.sample(texture_samplerSmplr, in.v_texture_coords);
            break;
        }
        float4 _1864;
        do
        {
            bool _796 = frag_info.tmx == 3.0;
            bool _809;
            if (_796)
            {
                bool _800 = in.v_texture_coords.x < 0.0;
                bool _807;
                if (!_800)
                {
                    _807 = in.v_texture_coords.x >= 1.0;
                }
                else
                {
                    _807 = _800;
                }
                _809 = _807;
            }
            else
            {
                _809 = _796;
            }
            bool _828;
            if (!_809)
            {
                bool _813 = frag_info.tmy == 3.0;
                bool _826;
                if (_813)
                {
                    bool _817 = in.v_texture_coords.y < 0.0;
                    bool _824;
                    if (!_817)
                    {
                        _824 = in.v_texture_coords.y >= 1.0;
                    }
                    else
                    {
                        _824 = _817;
                    }
                    _826 = _824;
                }
                else
                {
                    _826 = _813;
                }
                _828 = _826;
            }
            else
            {
                _828 = _809;
            }
            if (_828)
            {
                _1864 = float4(0.0);
                break;
            }
            _1864 = texture_sampler.sample(texture_samplerSmplr, in.v_texture_coords, bias(-0.4749999940395355224609375));
            break;
        } while(false);
        _1865 = _1864;
        break;
    } while(false);
    float4 _1866;
    do
    {
        if (_1865.w == 0.0)
        {
            _1866 = float4(0.0);
            break;
        }
        _1866 = float4(_1865.xyz / float3(_1865.w), _1865.w);
        break;
    } while(false);
    int _695 = int(frag_info.blend_mode - 14.0);
    float3 _1877;
    do
    {
        if (_695 == 7)
        {
            float3 _965 = _1866.xyz * 2.0;
            _1877 = mix(_1863.xyz - (((float3(1.0) - _965) * _1863.xyz) * (float3(1.0) - _1863.xyz)), _1863.xyz + ((_965 - float3(1.0)) * (mix(((((_1863.xyz * 16.0) - float3(12.0)) * _1863.xyz) + float3(4.0)) * _1863.xyz, sqrt(_1863.xyz), fast::max(sign(_1863.xyz - float3(0.25)), float3(0.0))) - _1863.xyz)), fast::max(sign(_1866.xyz - float3(0.5)), float3(0.0)));
            break;
        }
        if (_695 == 8)
        {
            _1877 = abs(_1863.xyz - _1866.xyz);
            break;
        }
        if (_695 == 9)
        {
            _1877 = (_1863.xyz + _1866.xyz) - ((_1863.xyz * 2.0) * _1866.xyz);
            break;
        }
        if (_695 == 10)
        {
            _1877 = _1863.xyz * _1866.xyz;
            break;
        }
        if (_695 == 11)
        {
            float _1102 = fast::min(fast::min(_1866.x, _1866.y), _1866.z);
            float _1110 = fast::max(fast::max(_1866.x, _1866.y), _1866.z);
            float3 _1874;
            if (_1102 < _1110)
            {
                _1874 = ((_1866.xyz - float3(_1102)) * (fast::max(fast::max(_1863.x, _1863.y), _1863.z) - fast::min(fast::min(_1863.x, _1863.y), _1863.z))) / float3(_1110 - _1102);
            }
            else
            {
                _1874 = float3(0.0);
            }
            float3 _1154 = _1874 + float3((((_1863.x * 0.300000011920928955078125) + (_1863.y * 0.589999973773956298828125)) + (_1863.z * 0.10999999940395355224609375)) - (((_1874.x * 0.300000011920928955078125) + (_1874.y * 0.589999973773956298828125)) + (_1874.z * 0.10999999940395355224609375)));
            float _1236 = _1154.x;
            float _1239 = _1154.y;
            float _1243 = _1154.z;
            float _1245 = ((_1236 * 0.300000011920928955078125) + (_1239 * 0.589999973773956298828125)) + (_1243 * 0.10999999940395355224609375);
            float _1184 = fast::min(fast::min(_1236, _1239), _1243);
            float _1192 = fast::max(fast::max(_1236, _1239), _1243);
            float3 _1875;
            if (_1184 < 0.0)
            {
                float3 _1199 = float3(_1245);
                _1875 = _1199 + (((_1154 - _1199) * _1245) / float3((_1245 - _1184) + 0.0009765625));
            }
            else
            {
                _1875 = _1154;
            }
            float3 _1876;
            if (_1192 > 1.0)
            {
                float3 _1218 = float3(_1245);
                _1876 = _1218 + (((_1875 - _1218) * (1.0 - _1245)) / float3((_1192 - _1245) + 0.0009765625));
            }
            else
            {
                _1876 = _1875;
            }
            _1877 = _1876;
            break;
        }
        if (_695 == 12)
        {
            float _1292 = fast::min(fast::min(_1863.x, _1863.y), _1863.z);
            float _1300 = fast::max(fast::max(_1863.x, _1863.y), _1863.z);
            float3 _1871;
            if (_1292 < _1300)
            {
                _1871 = ((_1863.xyz - float3(_1292)) * (fast::max(fast::max(_1866.x, _1866.y), _1866.z) - fast::min(fast::min(_1866.x, _1866.y), _1866.z))) / float3(_1300 - _1292);
            }
            else
            {
                _1871 = float3(0.0);
            }
            float3 _1344 = _1871 + float3((((_1863.x * 0.300000011920928955078125) + (_1863.y * 0.589999973773956298828125)) + (_1863.z * 0.10999999940395355224609375)) - (((_1871.x * 0.300000011920928955078125) + (_1871.y * 0.589999973773956298828125)) + (_1871.z * 0.10999999940395355224609375)));
            float _1426 = _1344.x;
            float _1429 = _1344.y;
            float _1433 = _1344.z;
            float _1435 = ((_1426 * 0.300000011920928955078125) + (_1429 * 0.589999973773956298828125)) + (_1433 * 0.10999999940395355224609375);
            float _1374 = fast::min(fast::min(_1426, _1429), _1433);
            float _1382 = fast::max(fast::max(_1426, _1429), _1433);
            float3 _1872;
            if (_1374 < 0.0)
            {
                float3 _1389 = float3(_1435);
                _1872 = _1389 + (((_1344 - _1389) * _1435) / float3((_1435 - _1374) + 0.0009765625));
            }
            else
            {
                _1872 = _1344;
            }
            float3 _1873;
            if (_1382 > 1.0)
            {
                float3 _1408 = float3(_1435);
                _1873 = _1408 + (((_1872 - _1408) * (1.0 - _1435)) / float3((_1382 - _1435) + 0.0009765625));
            }
            else
            {
                _1873 = _1872;
            }
            _1877 = _1873;
            break;
        }
        if (_695 == 13)
        {
            float3 _1470 = _1866.xyz + float3((((_1863.x * 0.300000011920928955078125) + (_1863.y * 0.589999973773956298828125)) + (_1863.z * 0.10999999940395355224609375)) - (((_1866.x * 0.300000011920928955078125) + (_1866.y * 0.589999973773956298828125)) + (_1866.z * 0.10999999940395355224609375)));
            float _1552 = _1470.x;
            float _1555 = _1470.y;
            float _1559 = _1470.z;
            float _1561 = ((_1552 * 0.300000011920928955078125) + (_1555 * 0.589999973773956298828125)) + (_1559 * 0.10999999940395355224609375);
            float _1500 = fast::min(fast::min(_1552, _1555), _1559);
            float _1508 = fast::max(fast::max(_1552, _1555), _1559);
            float3 _1869;
            if (_1500 < 0.0)
            {
                float3 _1515 = float3(_1561);
                _1869 = _1515 + (((_1470 - _1515) * _1561) / float3((_1561 - _1500) + 0.0009765625));
            }
            else
            {
                _1869 = _1470;
            }
            float3 _1870;
            if (_1508 > 1.0)
            {
                float3 _1534 = float3(_1561);
                _1870 = _1534 + (((_1869 - _1534) * (1.0 - _1561)) / float3((_1508 - _1561) + 0.0009765625));
            }
            else
            {
                _1870 = _1869;
            }
            _1877 = _1870;
            break;
        }
        if (_695 == 14)
        {
            float3 _1596 = _1863.xyz + float3((((_1866.x * 0.300000011920928955078125) + (_1866.y * 0.589999973773956298828125)) + (_1866.z * 0.10999999940395355224609375)) - (((_1863.x * 0.300000011920928955078125) + (_1863.y * 0.589999973773956298828125)) + (_1863.z * 0.10999999940395355224609375)));
            float _1678 = _1596.x;
            float _1681 = _1596.y;
            float _1685 = _1596.z;
            float _1687 = ((_1678 * 0.300000011920928955078125) + (_1681 * 0.589999973773956298828125)) + (_1685 * 0.10999999940395355224609375);
            float _1626 = fast::min(fast::min(_1678, _1681), _1685);
            float _1634 = fast::max(fast::max(_1678, _1681), _1685);
            float3 _1867;
            if (_1626 < 0.0)
            {
                float3 _1641 = float3(_1687);
                _1867 = _1641 + (((_1596 - _1641) * _1687) / float3((_1687 - _1626) + 0.0009765625));
            }
            else
            {
                _1867 = _1596;
            }
            float3 _1868;
            if (_1634 > 1.0)
            {
                float3 _1660 = float3(_1687);
                _1868 = _1660 + (((_1867 - _1660) * (1.0 - _1687)) / float3((_1634 - _1687) + 0.0009765625));
            }
            else
            {
                _1868 = _1867;
            }
            _1877 = _1868;
            break;
        }
        _1877 = float3(0.0);
        break;
    } while(false);
    float _1700 = _1866.w * _1863.w;
    float4 _1712 = float4(float4(_1877, _1700).xyz * _1700, _1700) + (float4(_1866.xyz * _1866.w, _1866.w) * (1.0 - _1863.w));
    out.frag_color = _1712 + (float4(_1863.xyz * _1863.w, _1863.w) * (1.0 - _1712.w));
    out.frag_color *= frag_info.alpha;
    return out;
}

