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
    float radius;
    float tile_mode;
    float4 decal_border_color;
    float alpha;
    float2 half_texel;
    float2 focus;
    float focus_radius;
};

struct imp_conical_gradient_fill_strip_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_conical_gradient_fill_strip_fragment_main_in
{
    float2 v_position [[user(locn0)]];
};

fragment imp_conical_gradient_fill_strip_fragment_main_out imp_conical_gradient_fill_strip_fragment_main(imp_conical_gradient_fill_strip_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], texture2d<float> texture_sampler [[texture(0)]], sampler texture_samplerSmplr [[sampler(0)]])
{
    imp_conical_gradient_fill_strip_fragment_main_out out = {};
    float2 _794;
    do
    {
        float _503 = frag_info.center.y - frag_info.focus.y;
        float _465 = frag_info.focus_radius / distance(frag_info.focus, frag_info.center);
        float3 _474 = (float3x3(float3(0.0, -1.0, 0.0), float3(1.0, 0.0, 0.0), float3(0.0, 0.0, 1.0)) * spvInverse3x3(float3x3(float3(_503, frag_info.focus.x - frag_info.center.x, 0.0), float3(frag_info.center.x - frag_info.focus.x, _503, 0.0), float3(frag_info.focus, 1.0)))) * float3(in.v_position, 1.0);
        float _478 = _474.y;
        float _482 = (_465 * _465) - (_478 * _478);
        if (_482 < 0.0)
        {
            _794 = float2(0.0, -1.0);
            break;
        }
        _794 = float2(_474.x + sqrt(_482), 1.0);
        break;
    } while(false);
    float4 _802;
    do
    {
        if (_794.y < 0.0)
        {
            _802 = float4(0.0);
            break;
        }
        float4 _801;
        do
        {
            bool _593 = frag_info.tile_mode == 3.0;
            bool _606;
            if (_593)
            {
                bool _597 = _794.x < 0.0;
                bool _604;
                if (!_597)
                {
                    _604 = _794.x >= 1.0;
                }
                else
                {
                    _604 = _597;
                }
                _606 = _604;
            }
            else
            {
                _606 = _593;
            }
            bool _625;
            if (!_606)
            {
                _625 = _593 ? false : _593;
            }
            else
            {
                _625 = _606;
            }
            if (_625)
            {
                _801 = frag_info.decal_border_color;
                break;
            }
            bool _657 = frag_info.tile_mode == 0.0;
            float _795;
            if (_657)
            {
                _795 = fast::clamp(_794.x, 0.0, 1.0);
            }
            else
            {
                float _796;
                if (frag_info.tile_mode == 1.0)
                {
                    _796 = fract(_794.x);
                }
                else
                {
                    float _797;
                    if (frag_info.tile_mode == 2.0)
                    {
                        float _672 = _794.x - 1.0;
                        _797 = abs((_672 - (2.0 * floor(_672 * 0.5))) - 1.0);
                    }
                    else
                    {
                        _797 = _794.x;
                    }
                    _796 = _797;
                }
                _795 = _796;
            }
            float _798;
            if (_657)
            {
                _798 = 0.5;
            }
            else
            {
                float _799;
                if (frag_info.tile_mode == 1.0)
                {
                    _799 = fract(0.5);
                }
                else
                {
                    float _800;
                    if (frag_info.tile_mode == 2.0)
                    {
                        _800 = abs((-1.5) - (2.0 * floor(-0.25)));
                    }
                    else
                    {
                        _800 = 0.5;
                    }
                    _799 = _800;
                }
                _798 = _799;
            }
            _801 = texture_sampler.sample(texture_samplerSmplr, float2(mix(frag_info.half_texel.x, 1.0 - frag_info.half_texel.x, _795), mix(frag_info.half_texel.y, 1.0 - frag_info.half_texel.y, _798)), bias(-0.4749999940395355224609375));
            break;
        } while(false);
        _802 = float4(_801.xyz * _801.w, _801.w) * frag_info.alpha;
        break;
    } while(false);
    out.frag_color = _802;
    return out;
}

