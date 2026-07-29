struct AudioLineUniform
{
    float4 g_ColorOpacity;
    float4 g_AmplitudeBandEnvelopeThickness;
    float4 g_SmoothnessVerticalOffset;
    float4 g_Unused;
    float4 g_Spectrum64Left[16];
    float4 g_Spectrum64Right[16];
};

[[vk::binding(0, 0)]] Sampler2D g_Texture0;
[[vk::binding(3, 0)]] ConstantBuffer<AudioLineUniform> u_Effect;

struct FragmentInput
{
    [[vk::location(0)]] float2 texCoord : TEXCOORD;
};

float spectrumLeft(int band)
{
    int index = clamp(band, 0, 63);
    return u_Effect.g_Spectrum64Left[index / 4][index % 4];
}

float spectrumRight(int band)
{
    int index = clamp(band, 0, 63);
    return u_Effect.g_Spectrum64Right[index / 4][index % 4];
}

float spectrum(int band)
{
    return 0.5 * (spectrumLeft(band) + spectrumRight(band));
}

float mirroredAudioValue(int index, int maximumBand)
{
    index = abs(index);
    if (index > maximumBand)
    {
        index = maximumBand - (index - maximumBand);
    }
    return spectrum(clamp(index, 0, 63));
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
    float width;
    float height;
    g_Texture0.GetDimensions(width, height);
    float aspect = width / max(height, 1.0);
    float2 uv = input.texCoord - 0.5;
    uv.x *= aspect;
    float xNormalized = abs(uv.x) / max(0.5 * aspect, 0.000001);
    float frequencyNormalized = 1.0 - xNormalized;
    float maximumBand = clamp(
        u_Effect.g_AmplitudeBandEnvelopeThickness.y, 1.0, 63.0);
    float audioIndex = frequencyNormalized * maximumBand;
    int index = int(floor(audioIndex));
    float t = frac(audioIndex);
    int maximumBandIndex = int(maximumBand);
    float rawAudio = cubicSpline(
        mirroredAudioValue(index - 1, maximumBandIndex),
        mirroredAudioValue(index, maximumBandIndex),
        mirroredAudioValue(index + 1, maximumBandIndex),
        mirroredAudioValue(index + 2, maximumBandIndex),
        t);
    rawAudio = max(0.0, rawAudio);
    float envelope = pow(
        max(0.0, 1.0 - xNormalized),
        u_Effect.g_AmplitudeBandEnvelopeThickness.z);
    float audio = rawAudio * envelope
        * u_Effect.g_AmplitudeBandEnvelopeThickness.x;
    float curveY = u_Effect.g_SmoothnessVerticalOffset.y - audio;
    float distanceToCurve = abs(uv.y - curveY);
    float halfThickness = u_Effect.g_AmplitudeBandEnvelopeThickness.w * 0.5;
    float line = 1.0 - smoothstep(
        halfThickness,
        halfThickness + u_Effect.g_SmoothnessVerticalOffset.x,
        distanceToCurve);
    float4 original = g_Texture0.Sample(input.texCoord);
    float curveAlpha = line * u_Effect.g_ColorOpacity.a;
    return float4(
        lerp(original.rgb, u_Effect.g_ColorOpacity.rgb, curveAlpha),
        max(original.a, curveAlpha));
}
