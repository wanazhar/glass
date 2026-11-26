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
    float2 warped = position - direction * amplitude * 9.0;
    half4 color = layer.sample(warped);
    half4 red = layer.sample(warped + direction * amplitude * 1.2);
    half4 blue = layer.sample(warped - direction * amplitude * 1.2);
    color.r = min(color.a, mix(color.r, red.r, half(0.35)));
    color.b = min(color.a, mix(color.b, blue.b, half(0.35)));
    return color;
}
