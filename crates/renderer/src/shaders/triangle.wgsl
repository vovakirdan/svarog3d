struct Camera {
    // Здесь теперь PV (OpenGL->WGPU преобразование применяется на CPU).
    mvp : mat4x4<f32>,
};
@group(0) @binding(0)
var<uniform> u_camera : Camera;

@group(1) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(1) @binding(1)
var s_diffuse: sampler;

struct VsIn {
    @location(0) pos    : vec3<f32>,
    @location(1) normal : vec3<f32>,
    @location(2) uv     : vec2<f32>,

    // per-instance model matrix columns:
    @location(3) i_col0 : vec4<f32>,
    @location(4) i_col1 : vec4<f32>,
    @location(5) i_col2 : vec4<f32>,
    @location(6) i_col3 : vec4<f32>,
};

struct VsOut {
    @builtin(position) pos : vec4<f32>,
    @location(0) normal : vec3<f32>,
    @location(1) uv : vec2<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    // WGSL mat4x4 ctor принимает **столбцы**
    let model = mat4x4<f32>(in.i_col0, in.i_col1, in.i_col2, in.i_col3);
    let normal_matrix = mat3x3<f32>(
        in.i_col0.xyz,
        in.i_col1.xyz,
        in.i_col2.xyz,
    );
    var out : VsOut;
    let pos4 = vec4<f32>(in.pos, 1.0);
    out.pos = u_camera.mvp * model * pos4;
    out.normal = normalize(normal_matrix * in.normal);
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let light_dir = normalize(vec3<f32>(-0.5, 1.0, -0.3));
    let ndotl = max(dot(in.normal, light_dir), 0.0);

    // Sample the diffuse texture
    let texture_color = textureSample(t_diffuse, s_diffuse, in.uv);

    // Apply simple directional lighting to the texture
    let lit_color = texture_color.rgb * (0.3 + 0.7 * ndotl);
    return vec4<f32>(lit_color, texture_color.a);
}
