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

struct imp_conical_gradient_fill_conical_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_conical_gradient_fill_conical_fragment_main_in
{
    float2 v_position [[user(locn0)]];
};

fragment imp_conical_gradient_fill_conical_fragment_main_out imp_conical_gradient_fill_conical_fragment_main(imp_conical_gradient_fill_conical_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], texture2d<float> texture_sampler [[texture(0)]], sampler texture_samplerSmplr [[sampler(0)]])
{
    imp_conical_gradient_fill_conical_fragment_main_out out = {};
    float2 _1102;
    do
    {
        float _619 = frag_info.focus_radius / (frag_info.focus_radius - frag_info.radius);
        bool _623 = abs(_619 - 1.0) < 0.000244140625;
        bool2 _1119 = bool2(_623);
        float2 _1120 = select(frag_info.center, frag_info.focus, _1119);
        float _1121 = _623 ? 0.0 : _619;
        float _632 = 1.0 - _1121;
        float2 _637 = (select(frag_info.focus, frag_info.center, _1119) * _632) + (_1120 * _1121);
        float _790 = _1120.y - _637.y;
        float _792 = _637.x;
        float _794 = _1120.x;
        float _643 = abs(_632);
        float _650 = abs(frag_info.radius - frag_info.focus_radius) / distance(frag_info.focus, frag_info.center);
        bool _654 = abs(_650 - 1.0) < 0.000244140625;
        float _1089;
        float _1090;
        if (_654)
        {
            float _658 = _643 * 0.5;
            _1090 = _658;
            _1089 = _658;
        }
        else
        {
            float _666 = (_650 * _650) - 1.0;
            _1090 = _643 / sqrt(abs(_666));
            _1089 = _643 * (_650 / _666);
        }
        float3 _692 = (float3x3(float3(_1089, 0.0, 0.0), float3(0.0, _1090, 0.0), float3(0.0, 0.0, 1.0)) * (float3x3(float3(0.0, -1.0, 0.0), float3(1.0, 0.0, 0.0), float3(0.0, 0.0, 1.0)) * spvInverse3x3(float3x3(float3(_790, _792 - _794, 0.0), float3(_794 - _792, _790, 0.0), float3(_792, _637.y, 1.0))))) * float3(in.v_position, 1.0);
        float2 _693 = _692.xy;
        float _695 = 1.0 / _650;
        float _698 = sign(_632);
        bool _703 = (!_654) && (_650 > 1.0);
        float _1093;
        if (_654)
        {
            _1093 = dot(_693, _693) / _692.x;
        }
        else
        {
            float _1094;
            if (_703)
            {
                _1094 = length(_693) - (_692.x * _695);
            }
            else
            {
                float _724 = _692.x;
                float _729 = _692.y;
                float _733 = (_724 * _724) - (_729 * _729);
                float _1095;
                if (_733 >= 0.0)
                {
                    float _1096;
                    if (_623 || (_698 < 0.0))
                    {
                        _1096 = (-sqrt(_733)) - (_724 * _695);
                    }
                    else
                    {
                        _1096 = sqrt(_733) - (_724 * _695);
                    }
                    _1095 = _1096;
                }
                else
                {
                    _1095 = -1.0;
                }
                _1094 = _1095;
            }
            _1093 = _1094;
        }
        if ((!_703) && (_1093 < 0.0))
        {
            _1102 = float2(0.0, -1.0);
            break;
        }
        float _773 = _1121 + (_698 * _1093);
        float _1101;
        if (_623)
        {
            _1101 = 1.0 - _773;
        }
        else
        {
            _1101 = _773;
        }
        _1102 = float2(_1101, 1.0);
        break;
    } while(false);
    float4 _1110;
    do
    {
        if (_1102.y < 0.0)
        {
            _1110 = float4(0.0);
            break;
        }
        float4 _1109;
        do
        {
            bool _880 = frag_info.tile_mode == 3.0;
            bool _893;
            if (_880)
            {
                bool _884 = _1102.x < 0.0;
                bool _891;
                if (!_884)
                {
                    _891 = _1102.x >= 1.0;
                }
                else
                {
                    _891 = _884;
                }
                _893 = _891;
            }
            else
            {
                _893 = _880;
            }
            bool _912;
            if (!_893)
            {
                _912 = _880 ? false : _880;
            }
            else
            {
                _912 = _893;
            }
            if (_912)
            {
                _1109 = frag_info.decal_border_color;
                break;
            }
            bool _944 = frag_info.tile_mode == 0.0;
            float _1103;
            if (_944)
            {
                _1103 = fast::clamp(_1102.x, 0.0, 1.0);
            }
            else
            {
                float _1104;
                if (frag_info.tile_mode == 1.0)
                {
                    _1104 = fract(_1102.x);
                }
                else
                {
                    float _1105;
                    if (frag_info.tile_mode == 2.0)
                    {
                        float _959 = _1102.x - 1.0;
                        _1105 = abs((_959 - (2.0 * floor(_959 * 0.5))) - 1.0);
                    }
                    else
                    {
                        _1105 = _1102.x;
                    }
                    _1104 = _1105;
                }
                _1103 = _1104;
            }
            float _1106;
            if (_944)
            {
                _1106 = 0.5;
            }
            else
            {
                float _1107;
                if (frag_info.tile_mode == 1.0)
                {
                    _1107 = fract(0.5);
                }
                else
                {
                    float _1108;
                    if (frag_info.tile_mode == 2.0)
                    {
                        _1108 = abs((-1.5) - (2.0 * floor(-0.25)));
                    }
                    else
                    {
                        _1108 = 0.5;
                    }
                    _1107 = _1108;
                }
                _1106 = _1107;
            }
            _1109 = texture_sampler.sample(texture_samplerSmplr, float2(mix(frag_info.half_texel.x, 1.0 - frag_info.half_texel.x, _1103), mix(frag_info.half_texel.y, 1.0 - frag_info.half_texel.y, _1106)), bias(-0.4749999940395355224609375));
            break;
        } while(false);
        _1110 = float4(_1109.xyz * _1109.w, _1109.w) * frag_info.alpha;
        break;
    } while(false);
    out.frag_color = _1110;
    return out;
}

