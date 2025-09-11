// B3: MVP + color, depth-enabled.

// Camera UBO
struct Camera {
    mvp : mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> uCamera : Camera;

struct VsOut {
    @builtin(position) pos : vec4<f32>,
    @location(0) color : vec3<f32>,
};

@vertex
fn vs_main(@location(0) in_pos: vec3<f32>, @location(1) in_color: vec3<f32>) -> VsOut {
    var out: VsOut;
    out.pos = uCamera.mvp * vec4<f32>(in_pos, 1.0);
    out.color = in_color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
