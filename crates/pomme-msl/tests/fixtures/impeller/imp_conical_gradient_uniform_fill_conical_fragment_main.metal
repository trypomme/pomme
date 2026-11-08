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

struct FragInfo
{
    float2 center;
    float2 focus;
    float focus_radius;
    float radius;
    float tile_mode;
    float alpha;
    float colors_length;
    float4 decal_border_color;
};

struct StopPairsInfo
{
    float4 stop_pairs[128];
};

struct ColorsInfo
{
    float4 colors[256];
};

struct imp_conical_gradient_uniform_fill_conical_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_conical_gradient_uniform_fill_conical_fragment_main_in
{
    float2 v_position [[user(locn0)]];
};

fragment imp_conical_gradient_uniform_fill_conical_fragment_main_out imp_conical_gradient_uniform_fill_conical_fragment_main(imp_conical_gradient_uniform_fill_conical_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], constant StopPairsInfo& stop_pairs_info [[buffer(1)]], constant ColorsInfo& colors_info [[buffer(2)]])
{
    imp_conical_gradient_uniform_fill_conical_fragment_main_out out = {};
    float2 _978;
    do
    {
        float _567 = frag_info.focus_radius / (frag_info.focus_radius - frag_info.radius);
        bool _571 = abs(_567 - 1.0) < 0.000244140625;
        bool2 _1010 = bool2(_571);
        float2 _1011 = select(frag_info.center, frag_info.focus, _1010);
        float _1012 = _571 ? 0.0 : _567;
        float _580 = 1.0 - _1012;
        float2 _585 = (select(frag_info.focus, frag_info.center, _1010) * _580) + (_1011 * _1012);
        float _738 = _1011.y - _585.y;
        float _740 = _585.x;
        float _742 = _1011.x;
        float _591 = abs(_580);
        float _598 = abs(frag_info.radius - frag_info.focus_radius) / distance(frag_info.focus, frag_info.center);
        bool _602 = abs(_598 - 1.0) < 0.000244140625;
        float _965;
        float _966;
        if (_602)
        {
            float _606 = _591 * 0.5;
            _966 = _606;
            _965 = _606;
        }
        else
        {
            float _614 = (_598 * _598) - 1.0;
            _966 = _591 / sqrt(abs(_614));
            _965 = _591 * (_598 / _614);
        }
        float3 _640 = (float3x3(float3(_965, 0.0, 0.0), float3(0.0, _966, 0.0), float3(0.0, 0.0, 1.0)) * (float3x3(float3(0.0, -1.0, 0.0), float3(1.0, 0.0, 0.0), float3(0.0, 0.0, 1.0)) * spvInverse3x3(float3x3(float3(_738, _740 - _742, 0.0), float3(_742 - _740, _738, 0.0), float3(_740, _585.y, 1.0))))) * float3(in.v_position, 1.0);
        float2 _641 = _640.xy;
        float _643 = 1.0 / _598;
        float _646 = sign(_580);
        bool _651 = (!_602) && (_598 > 1.0);
        float _969;
        if (_602)
        {
            _969 = dot(_641, _641) / _640.x;
        }
        else
        {
            float _970;
            if (_651)
            {
                _970 = length(_641) - (_640.x * _643);
            }
            else
            {
                float _672 = _640.x;
                float _677 = _640.y;
                float _681 = (_672 * _672) - (_677 * _677);
                float _971;
                if (_681 >= 0.0)
                {
                    float _972;
                    if (_571 || (_646 < 0.0))
                    {
                        _972 = (-sqrt(_681)) - (_672 * _643);
                    }
                    else
                    {
                        _972 = sqrt(_681) - (_672 * _643);
                    }
                    _971 = _972;
                }
                else
                {
                    _971 = -1.0;
                }
                _970 = _971;
            }
            _969 = _970;
        }
        if ((!_651) && (_969 < 0.0))
        {
            _978 = float2(0.0, -1.0);
            break;
        }
        float _721 = _1012 + (_646 * _969);
        float _977;
        if (_571)
        {
            _977 = 1.0 - _721;
        }
        else
        {
            _977 = _721;
        }
        _978 = float2(_977, 1.0);
        break;
    } while(false);
    bool _785 = _978.y < 0.0;
    bool _800;
    if (!_785)
    {
        bool _792 = (_978.x < 0.0) || (_978.x > 1.0);
        bool _798;
        if (_792)
        {
            _798 = frag_info.tile_mode == 3.0;
        }
        else
        {
            _798 = _792;
        }
        _800 = _798;
    }
    else
    {
        _800 = _785;
    }
    float4 _996;
    if (_800)
    {
        _996 = frag_info.decal_border_color;
    }
    else
    {
        float _981;
        if (frag_info.tile_mode == 0.0)
        {
            _981 = fast::clamp(_978.x, 0.0, 1.0);
        }
        else
        {
            float _982;
            if (frag_info.tile_mode == 1.0)
            {
                _982 = fract(_978.x);
            }
            else
            {
                float _983;
                if (frag_info.tile_mode == 2.0)
                {
                    float _908 = _978.x - 1.0;
                    _983 = abs((_908 - (2.0 * floor(_908 * 0.5))) - 1.0);
                }
                else
                {
                    _983 = _978.x;
                }
                _982 = _983;
            }
            _981 = _982;
        }
        float2 _991;
        _991 = stop_pairs_info.stop_pairs[0].xy;
        bool _836;
        float2 _986;
        float4 _997;
        int _984 = 1;
        bool _985 = false;
        for (;;)
        {
            if (float(_984) < frag_info.colors_length)
            {
                if (_985)
                {
                    _986 = stop_pairs_info.stop_pairs[_984 / 2].xy;
                }
                else
                {
                    _986 = stop_pairs_info.stop_pairs[_984 / 2].zw;
                }
                _836 = !_985;
                bool _840 = _981 >= _991.x;
                bool _847;
                if (_840)
                {
                    _847 = _981 <= _986.x;
                }
                else
                {
                    _847 = _840;
                }
                if (_847)
                {
                    float4 _1006;
                    if (_986.y > 1000.0)
                    {
                        _1006 = colors_info.colors[_984];
                    }
                    else
                    {
                        _1006 = mix(colors_info.colors[_984 - 1], colors_info.colors[_984], float4((_981 - _991.x) * _986.y));
                    }
                    _997 = _1006;
                    break;
                }
                _991 = _986;
                _985 = _836;
                _984++;
                continue;
            }
            else
            {
                _997 = float4(0.0);
                break;
            }
        }
        _996 = _997;
    }
    out.frag_color = float4(_996.xyz * _996.w, _996.w) * frag_info.alpha;
    return out;
}

