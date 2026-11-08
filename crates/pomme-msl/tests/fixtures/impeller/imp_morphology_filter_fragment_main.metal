#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float2 uv_offset;
    float radius;
    float morph_type;
    float supports_decal_sampler_address_mode;
};

constant float supports_decal_tmp [[function_constant(0)]];
constant float supports_decal = is_function_constant_defined(supports_decal_tmp) ? supports_decal_tmp : 1.0;

struct imp_morphology_filter_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_morphology_filter_fragment_main_in
{
    float2 v_texture_coords [[user(locn0)]];
};

fragment imp_morphology_filter_fragment_main_out imp_morphology_filter_fragment_main(imp_morphology_filter_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], texture2d<float> texture_sampler [[texture(0)]], sampler texture_samplerSmplr [[sampler(0)]])
{
    imp_morphology_filter_fragment_main_out out = {};
    bool _60 = frag_info.morph_type == 0.0;
    float _70 = -frag_info.radius;
    float4 _162;
    _162 = select(float4(1.0), float4(0.0), bool4(_60));
    float4 _172;
    for (float _161 = _70; _161 <= frag_info.radius; _162 = _172, _161 += 1.0)
    {
        float2 _90 = in.v_texture_coords + (frag_info.uv_offset * _161);
        float4 _164;
        if (supports_decal == 1.0)
        {
            _164 = texture_sampler.sample(texture_samplerSmplr, _90);
        }
        else
        {
            float4 _163;
            do
            {
                bool _146 = any(_90 < float2(0.0));
                bool _153;
                if (!_146)
                {
                    _153 = any(_90 >= float2(1.0));
                }
                else
                {
                    _153 = _146;
                }
                if (_153)
                {
                    _163 = float4(0.0);
                    break;
                }
                _163 = texture_sampler.sample(texture_samplerSmplr, _90, bias(-0.4749999940395355224609375));
                break;
            } while(false);
            _164 = _163;
        }
        if (_60)
        {
            _172 = fast::max(_164, _162);
        }
        else
        {
            _172 = fast::min(_164, _162);
        }
    }
    out.frag_color = _162;
    return out;
}

