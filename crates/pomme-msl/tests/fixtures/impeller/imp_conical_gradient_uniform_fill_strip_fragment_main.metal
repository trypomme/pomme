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

struct imp_conical_gradient_uniform_fill_strip_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_conical_gradient_uniform_fill_strip_fragment_main_in
{
    float2 v_position [[user(locn0)]];
};

fragment imp_conical_gradient_uniform_fill_strip_fragment_main_out imp_conical_gradient_uniform_fill_strip_fragment_main(imp_conical_gradient_uniform_fill_strip_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], constant StopPairsInfo& stop_pairs_info [[buffer(1)]], constant ColorsInfo& colors_info [[buffer(2)]])
{
    imp_conical_gradient_uniform_fill_strip_fragment_main_out out = {};
    float2 _670;
    do
    {
        float _451 = frag_info.center.y - frag_info.focus.y;
        float _413 = frag_info.focus_radius / distance(frag_info.focus, frag_info.center);
        float3 _422 = (float3x3(float3(0.0, -1.0, 0.0), float3(1.0, 0.0, 0.0), float3(0.0, 0.0, 1.0)) * spvInverse3x3(float3x3(float3(_451, frag_info.focus.x - frag_info.center.x, 0.0), float3(frag_info.center.x - frag_info.focus.x, _451, 0.0), float3(frag_info.focus, 1.0)))) * float3(in.v_position, 1.0);
        float _426 = _422.y;
        float _430 = (_413 * _413) - (_426 * _426);
        if (_430 < 0.0)
        {
            _670 = float2(0.0, -1.0);
            break;
        }
        _670 = float2(_422.x + sqrt(_430), 1.0);
        break;
    } while(false);
    bool _498 = _670.y < 0.0;
    bool _513;
    if (!_498)
    {
        bool _505 = (_670.x < 0.0) || (_670.x > 1.0);
        bool _511;
        if (_505)
        {
            _511 = frag_info.tile_mode == 3.0;
        }
        else
        {
            _511 = _505;
        }
        _513 = _511;
    }
    else
    {
        _513 = _498;
    }
    float4 _688;
    if (_513)
    {
        _688 = frag_info.decal_border_color;
    }
    else
    {
        float _673;
        if (frag_info.tile_mode == 0.0)
        {
            _673 = fast::clamp(_670.x, 0.0, 1.0);
        }
        else
        {
            float _674;
            if (frag_info.tile_mode == 1.0)
            {
                _674 = fract(_670.x);
            }
            else
            {
                float _675;
                if (frag_info.tile_mode == 2.0)
                {
                    float _621 = _670.x - 1.0;
                    _675 = abs((_621 - (2.0 * floor(_621 * 0.5))) - 1.0);
                }
                else
                {
                    _675 = _670.x;
                }
                _674 = _675;
            }
            _673 = _674;
        }
        float2 _683;
        _683 = stop_pairs_info.stop_pairs[0].xy;
        bool _549;
        float2 _678;
        float4 _689;
        int _676 = 1;
        bool _677 = false;
        for (;;)
        {
            if (float(_676) < frag_info.colors_length)
            {
                if (_677)
                {
                    _678 = stop_pairs_info.stop_pairs[_676 / 2].xy;
                }
                else
                {
                    _678 = stop_pairs_info.stop_pairs[_676 / 2].zw;
                }
                _549 = !_677;
                bool _553 = _673 >= _683.x;
                bool _560;
                if (_553)
                {
                    _560 = _673 <= _678.x;
                }
                else
                {
                    _560 = _553;
                }
                if (_560)
                {
                    float4 _698;
                    if (_678.y > 1000.0)
                    {
                        _698 = colors_info.colors[_676];
                    }
                    else
                    {
                        _698 = mix(colors_info.colors[_676 - 1], colors_info.colors[_676], float4((_673 - _683.x) * _678.y));
                    }
                    _689 = _698;
                    break;
                }
                _683 = _678;
                _677 = _549;
                _676++;
                continue;
            }
            else
            {
                _689 = float4(0.0);
                break;
            }
        }
        _688 = _689;
    }
    out.frag_color = float4(_688.xyz * _688.w, _688.w) * frag_info.alpha;
    return out;
}

