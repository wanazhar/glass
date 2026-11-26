#include <metal_stdlib>
#include <SwiftUI/SwiftUI.h>
using namespace metal;

[[ stitchable ]] half4 liquidRipple(float2 position, SwiftUI::Layer layer,
                                    float2 touch, float time) {
    if (time <= 0.0 || time >= 1.3) return layer.sample(position);
    float2 delta = position - touch;
    float distance = length(delta);
    float2 direction = delta / max(distance, 1.0);
    float phase = time - distance / 250.0;
    if (phase <= 0.0) return layer.sample(position);
    float amplitude = sin(phase * 31.0) * exp(-phase * 7.0);
    amplitude *= smoothstep(0.0, 0.045, phase) * (1.0 - smoothstep(0.85, 1.3, time));
    float2 warped = position - direction * amplitude * 7.0;
    float2 dispersion = direction * abs(amplitude) * 5.0;
    half4 ink = layer.sample(warped);
    half red = layer.sample(warped - dispersion).a;
    half blue = layer.sample(warped + dispersion).a;
    half redEdge = max(half(0.0), red - max(ink.a, blue));
    half blueEdge = max(half(0.0), blue - max(ink.a, red));
    half4 fringe = half4(half3(1.0, 0.20, 0.36) * redEdge +
                         half3(0.20, 0.51, 1.0) * blueEdge, redEdge + blueEdge) * half(0.68);
    return ink + fringe * (half(1.0) - ink.a);
}
