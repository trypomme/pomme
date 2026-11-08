#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

constant float supports_decal_tmp [[function_constant(0)]];
constant float supports_decal = is_function_constant_defined(supports_decal_tmp) ? supports_decal_tmp : 1.0;

struct FragInfo
{
    float unpremultiply;
    float sample_count;
};

struct KernelSamples
{
    float4 sample_data[50];
};

struct imp_gaussian_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_gaussian_fragment_main_in
{
    float2 v_texture_coords [[user(locn0)]];
};

fragment imp_gaussian_fragment_main_out imp_gaussian_fragment_main(imp_gaussian_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], constant KernelSamples& kernel_samples [[buffer(1)]], texture2d<float> texture_sampler [[texture(0)]], sampler texture_samplerSmplr [[sampler(0)]])
{
    imp_gaussian_fragment_main_out out = {};
    float4 _236;
    _236 = float4(0.0);
    float4 _135;
    for (int _235 = 0; _235 < int(frag_info.sample_count); _236 = _135, _235++)
    {
        float2 _130 = in.v_texture_coords + kernel_samples.sample_data[_235].xy;
        float4 _239;
        do
        {
            if (supports_decal == 1.0)
            {
                _239 = texture_sampler.sample(texture_samplerSmplr, _130);
                break;
            }
            float4 _238;
            do
            {
                bool _201 = any(_130 < float2(0.0));
                bool _208;
                if (!_201)
                {
                    _208 = any(_130 >= float2(1.0));
                }
                else
                {
                    _208 = _201;
                }
                if (_208)
                {
                    _238 = float4(0.0);
                    break;
                }
                _238 = texture_sampler.sample(texture_samplerSmplr, _130, bias(-0.4749999940395355224609375));
                break;
            } while(false);
            _239 = _238;
            break;
        } while(false);
        _135 = _236 + (_239 * kernel_samples.sample_data[_235].z);
    }
    if (frag_info.unpremultiply > 0.5)
    {
        float4 _237;
        do
        {
            if (_236.w == 0.0)
            {
                _237 = float4(0.0);
                break;
            }
            _237 = _236 / float4(_236.w);
            break;
        } while(false);
        out.frag_color = _237;
    }
    else
    {
        out.frag_color = _236;
    }
    return out;
}

