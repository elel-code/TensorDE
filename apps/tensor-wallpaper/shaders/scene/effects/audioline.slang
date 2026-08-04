struct AudioLineUniform
{
    float4 g_ColorOpacity;
    float4 g_AmplitudeBandEnvelopeThickness;
    float4 g_SmoothnessVerticalOffset;
    float4 g_Unused;
    float4 g_Spectrum64Left[16];
    float4 g_Spectrum64Right[16];
};

struct AudioLinePush
{
    uint textureIndex;
    uint samplerIndex;
    uint materialIndex;
};

[[vk::push_constant]] ConstantBuffer<AudioLinePush> push;

struct FragmentInput
{
    [[vk::location(0)]] float2 texCoord : TEXCOORD;
};

float spectrumLeft(ConstantBuffer<AudioLineUniform> effect, int band)
{
    int index = clamp(band, 0, 63);
    return effect.g_Spectrum64Left[index / 4][index % 4];
}

float spectrumRight(ConstantBuffer<AudioLineUniform> effect, int band)
{
    int index = clamp(band, 0, 63);
    return effect.g_Spectrum64Right[index / 4][index % 4];
}

float spectrum(ConstantBuffer<AudioLineUniform> effect, int band)
{
    return 0.5 * (spectrumLeft(effect, band) + spectrumRight(effect, band));
}

float mirroredAudioValue(
    ConstantBuffer<AudioLineUniform> effect,
    int index,
    int maximumBand)
{
    index = abs(index);
    if (index > maximumBand)
    {
        index = maximumBand - (index - maximumBand);
    }
    return spectrum(effect, clamp(index, 0, 63));
}

float cubicSpline(float p0, float p1, float p2, float p3, float t)
{
    float t2 = t * t;
    float t3 = t2 * t;
    return 0.5 * ((2.0 * p1) + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3);
}

[shader("fragment")]
float4 fragmentMain(FragmentInput input) : SV_Target0
{
    Texture2D<float4> texture =
        DescriptorHandle<Texture2D<float4>>(push.textureIndex);
    SamplerState sampler =
        DescriptorHandle<SamplerState>(push.samplerIndex);
    ConstantBuffer<AudioLineUniform> effect =
        DescriptorHandle<ConstantBuffer<AudioLineUniform>>(push.materialIndex);
    float width;
    float height;
    texture.GetDimensions(width, height);
    float aspect = width / max(height, 1.0);
    float2 uv = input.texCoord - 0.5;
    uv.x *= aspect;
    float xNormalized = abs(uv.x) / max(0.5 * aspect, 0.000001);
    float frequencyNormalized = 1.0 - xNormalized;
    float maximumBand = clamp(
        effect.g_AmplitudeBandEnvelopeThickness.y, 1.0, 63.0);
    float audioIndex = frequencyNormalized * maximumBand;
    int index = int(floor(audioIndex));
    float t = frac(audioIndex);
    int maximumBandIndex = int(maximumBand);
    float rawAudio = cubicSpline(
        mirroredAudioValue(effect, index - 1, maximumBandIndex),
        mirroredAudioValue(effect, index, maximumBandIndex),
        mirroredAudioValue(effect, index + 1, maximumBandIndex),
        mirroredAudioValue(effect, index + 2, maximumBandIndex),
        t);
    rawAudio = max(0.0, rawAudio);
    float envelope = pow(
        max(0.0, 1.0 - xNormalized),
        effect.g_AmplitudeBandEnvelopeThickness.z);
    float audio = rawAudio * envelope
        * effect.g_AmplitudeBandEnvelopeThickness.x;
    float curveY = effect.g_SmoothnessVerticalOffset.y - audio;
    float distanceToCurve = abs(uv.y - curveY);
    float halfThickness = effect.g_AmplitudeBandEnvelopeThickness.w * 0.5;
    float line = 1.0 - smoothstep(
        halfThickness,
        halfThickness + effect.g_SmoothnessVerticalOffset.x,
        distanceToCurve);
    float4 original = texture.Sample(sampler, input.texCoord);
    float curveAlpha = line * effect.g_ColorOpacity.a;
    return float4(
        lerp(original.rgb, effect.g_ColorOpacity.rgb, curveAlpha),
        max(original.a, curveAlpha));
}
