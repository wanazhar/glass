#include <metal_stdlib>
#include <SwiftUI/SwiftUI_Metal.h>
using namespace metal;

[[ stitchable ]] half4 refraction(float2 position, SwiftUI::Layer layer, float2 origin,
                                   float elapsed, float duration, float2 size) {
    float2 delta = position - origin;
    float radius = length(delta);
    float2 normal = delta / max(radius, 1.0);
    float t = clamp(elapsed / duration, 0.0, 1.0);
    float front = t * max(size.x, size.y) * 1.18;
    float edge = radius - front;
    float band = exp(-pow(edge / (22.0 + 12.0 * t), 2.0));
    float energy = band * smoothstep(0.0, 0.085, t) * (1.0 - smoothstep(0.38, 1.0, t));
    float bend = sin(edge * 0.12) * energy * 24.0;
    float2 source = clamp(position + normal * bend, float2(0.5), size - 0.5);
    half4 sample = layer.sample(source);
    sample.rgb += half(cos(edge * 0.12) * energy * 0.047) * sample.a;
    return sample;
}
