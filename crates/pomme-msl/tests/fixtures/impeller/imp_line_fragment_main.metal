#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float4 color;
    float cap_type;
};

struct imp_line_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_line_fragment_main_in
{
    float2 v_position [[user(locn0)]];
    float3 v_e0 [[user(locn1)]];
    float3 v_e1 [[user(locn2)]];
    float3 v_e2 [[user(locn3)]];
    float3 v_e3 [[user(locn4)]];
};

fragment imp_line_fragment_main_out imp_line_fragment_main(imp_line_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], texture2d<float> curve [[texture(0)]], sampler curveSmplr [[sampler(0)]])
{
    imp_line_fragment_main_out out = {};
    float _334;
    do
    {
        float3 _208 = float3(in.v_position, 1.0);
        float _211 = dot(_208, in.v_e0);
        float _214 = dot(_208, in.v_e1);
        float _217 = dot(_208, in.v_e2);
        float _220 = dot(_208, in.v_e3);
        if (any(float4(_211, _214, _217, _220) < float4(0.0)))
        {
            _334 = 0.0;
            break;
        }
        if (frag_info.cap_type == 1.0)
        {
            float _235 = fast::min(_214, _220);
            if (_235 < 1.0)
            {
                _334 = curve.sample(curveSmplr, float2(fast::clamp(1.0 - distance(float2(fast::min(_211, _217), _235), float2(1.0)), 0.0, 1.0), 0.0)).x;
                break;
            }
            else
            {
                _334 = curve.sample(curveSmplr, float2(fast::min(_211, _217), 0.0)).x;
                break;
            }
            break; // unreachable workaround
        }
        else
        {
            _334 = curve.sample(curveSmplr, float2(fast::min(_211, _217), 0.0)).x * curve.sample(curveSmplr, float2(fast::min(_214, _220), 0.0)).x;
            break;
        }
        break; // unreachable workaround
    } while(false);
    out.frag_color = float4(frag_info.color.xyz, _334);
    out.frag_color = float4(out.frag_color.xyz * out.frag_color.w, out.frag_color.w);
    return out;
}

