// A picture with CSS `filter: saturate(s) brightness(b)` (docs/architecture/client/ui.md, Filter).
//
// The filter functions as the browser applies them: on sRGB values, saturate with the Filter
// Effects matrix (a mix toward the 0.213/0.715/0.072 luma), then brightness as a multiply. The
// picture is stretched over the node, as an `ImageNode` would be, and cut to the node's corner
// radius (overflow clipping in bevy_ui is a rectangle, so a parent's radius does not round it).

struct FilterUniform {
    // x saturate, y brightness, z w unused
    p: vec4<f32>,
}

@group(1) @binding(0) var<uniform> params: FilterUniform;
@group(1) @binding(1) var picture: texture_2d<f32>;
@group(1) @binding(2) var picture_sampler: sampler;

struct UiVertexOutput {
    @location(0) uv: vec2<f32>,
    @location(1) border_widths: vec4<f32>,
    @location(2) border_radius_x: vec4<f32>,
    @location(3) border_radius_y: vec4<f32>,
    @location(4) @interpolate(flat) size: vec2<f32>,
    @builtin(position) position: vec4<f32>,
};

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055);
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(picture, picture_sampler, in.uv);
    let c = to_srgb(texel.rgb);
    let luma = vec3<f32>(dot(c, vec3<f32>(0.213, 0.715, 0.072)));
    let filtered = clamp(mix(luma, c, params.p.x) * params.p.y, vec3<f32>(0.0), vec3<f32>(1.0));
    // The node's corner radius (one for all corners: the top left's), antialiased over a pixel.
    let half = in.size * 0.5;
    let r = min(in.border_radius_x.x, min(half.x, half.y));
    let q = abs(in.uv * in.size - half) - half + vec2<f32>(r);
    let d = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
    let inside = clamp(0.5 - d, 0.0, 1.0);
    return vec4<f32>(to_linear(filtered), texel.a * inside);
}
