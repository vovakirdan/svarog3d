// B2: Minimal vertex+fragment shader with per-vertex color.

struct VsOut {
    @builtin(position) pos : vec4<f32>,
    @location(0) color : vec3<f32>,
};

@vertex
fn vs_main(@location(0) in_pos: vec3<f32>, @location(1) in_color: vec3<f32>) -> VsOut {
    var out: VsOut;
    out.pos = vec4<f32>(in_pos, 1.0);
    out.color = in_color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
