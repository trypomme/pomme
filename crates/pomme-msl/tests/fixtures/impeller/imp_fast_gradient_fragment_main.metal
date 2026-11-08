#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float alpha;
};

struct imp_fast_gradient_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_fast_gradient_fragment_main_in
{
    float4 v_color [[user(locn0)]];
};

fragment imp_fast_gradient_fragment_main_out imp_fast_gradient_fragment_main(imp_fast_gradient_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], float4 gl_FragCoord [[position]])
{
    imp_fast_gradient_fragment_main_out out = {};
    out.frag_color = float4(in.v_color.xyz * in.v_color.w, in.v_color.w) * frag_info.alpha;
    float4 _129 = out.frag_color;
    uint _163 = uint(gl_FragCoord.x) % 8u;
    uint _169 = uint(gl_FragCoord.y) ^ _163;
    float3 _202 = _129.xyz + float3(((float(((((((_169 & 1u) << uint(5)) | ((_163 & 1u) << uint(4))) | ((_169 & 2u) << uint(2))) | ((_163 & 2u) << uint(1))) | ((_169 & 4u) >> uint(1))) | ((_163 & 4u) >> uint(2))) * 0.015625) - 0.4921875) * 0.015625);
    float4 _215 = _129;
    _215.x = _202.x;
    _215.y = _202.y;
    _215.z = _202.z;
    out.frag_color = _215;
    return out;
}

