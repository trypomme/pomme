#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float src_factor;
    float inner_blur_factor;
    float outer_blur_factor;
    float alpha;
    float2 sigma_uv;
};

struct imp_border_mask_blur_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_border_mask_blur_fragment_main_in
{
    float2 v_texture_coords [[user(locn0)]];
};

fragment imp_border_mask_blur_fragment_main_out imp_border_mask_blur_fragment_main(imp_border_mask_blur_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], texture2d<float> texture_sampler [[texture(0)]], sampler texture_samplerSmplr [[sampler(0)]])
{
    imp_border_mask_blur_fragment_main_out out = {};
    float4 _130 = texture_sampler.sample(texture_samplerSmplr, in.v_texture_coords);
    float _239 = 0.707106769084930419921875 / frag_info.sigma_uv.x;
    float _240 = in.v_texture_coords.x * _239;
    float _249 = abs(_240);
    float _260 = ((0.2783930003643035888671875 + ((0.23038899898529052734375 + ((0.07810799777507781982421875 * _249) * _249)) * _249)) * _249) + 1.0;
    float _278 = 0.707106769084930419921875 / frag_info.sigma_uv.y;
    float _279 = in.v_texture_coords.y * _278;
    float _288 = abs(_279);
    float _299 = ((0.2783930003643035888671875 + ((0.23038899898529052734375 + ((0.07810799777507781982421875 * _288) * _288)) * _288)) * _288) + 1.0;
    float _318 = (1.0 - in.v_texture_coords.x) * _239;
    float _327 = abs(_318);
    float _338 = ((0.2783930003643035888671875 + ((0.23038899898529052734375 + ((0.07810799777507781982421875 * _327) * _327)) * _327)) * _327) + 1.0;
    float _357 = (1.0 - in.v_texture_coords.y) * _278;
    float _366 = abs(_357);
    float _377 = ((0.2783930003643035888671875 + ((0.23038899898529052734375 + ((0.07810799777507781982421875 * _366) * _366)) * _366)) * _366) + 1.0;
    float _233 = ((((1.0 + (sign(_240) * (1.0 - (1.0 / (((_260 * _260) * _260) * _260))))) * 0.5) * ((1.0 + (sign(_279) * (1.0 - (1.0 / (((_299 * _299) * _299) * _299))))) * 0.5)) * ((1.0 + (sign(_318) * (1.0 - (1.0 / (((_338 * _338) * _338) * _338))))) * 0.5)) * ((1.0 + (sign(_357) * (1.0 - (1.0 / (((_377 * _377) * _377) * _377))))) * 0.5);
    bool _141 = in.v_texture_coords.x >= 0.0;
    bool _147;
    if (_141)
    {
        _147 = in.v_texture_coords.y >= 0.0;
    }
    else
    {
        _147 = _141;
    }
    bool _153;
    if (_147)
    {
        _153 = in.v_texture_coords.x < 1.0;
    }
    else
    {
        _153 = _147;
    }
    bool _159;
    if (_153)
    {
        _159 = in.v_texture_coords.y < 1.0;
    }
    else
    {
        _159 = _153;
    }
    float _160 = float(_159);
    out.frag_color = _130 * ((((frag_info.inner_blur_factor * _233) + frag_info.src_factor) * _160) + ((frag_info.outer_blur_factor * _233) * (1.0 - _160)));
    return out;
}

