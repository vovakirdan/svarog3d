struct Camera {
    // Здесь теперь PV (OpenGL->WGPU преобразование применяется на CPU).
    mvp : mat4x4<f32>,
};
@group(0) @binding(0)
var<uniform> u_camera : Camera;

struct VsIn {
    @location(0) pos   : vec3<f32>,
    @location(1) color : vec3<f32>,

    // per-instance model matrix columns:
    @location(2) i_col0 : vec4<f32>,
    @location(3) i_col1 : vec4<f32>,
    @location(4) i_col2 : vec4<f32>,
    @location(5) i_col3 : vec4<f32>,
};

struct VsOut {
    @builtin(position) pos : vec4<f32>,
    @location(0) color : vec3<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    // WGSL mat4x4 ctor принимает **столбцы**
    let model = mat4x4<f32>(in.i_col0, in.i_col1, in.i_col2, in.i_col3);
    var out : VsOut;
    let pos4 = vec4<f32>(in.pos, 1.0);
    out.pos = u_camera.mvp * model * pos4;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
