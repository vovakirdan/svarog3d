struct Camera {
    // Здесь теперь PV (OpenGL->WGPU преобразование применяется на CPU).
    mvp : mat4x4<f32>,
};
@group(0) @binding(0)
var<uniform> u_camera : Camera;

struct Material {
    base_color: vec4<f32>,
    metallic_roughness: vec2<f32>,
};

struct Lighting {
    light_direction: vec3<f32>,
    light_intensity: f32,
    light_color: vec3<f32>,
    ambient_intensity: f32,
};

@group(1) @binding(0)
var<uniform> u_material : Material;
@group(1) @binding(1)
var<uniform> u_lighting : Lighting;

@group(2) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(2) @binding(1)
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
    @location(0) world_pos : vec3<f32>,
    @location(1) normal : vec3<f32>,
    @location(2) uv : vec2<f32>,
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
    let world_pos4 = model * pos4;
    out.pos = u_camera.mvp * world_pos4;
    out.world_pos = world_pos4.xyz;
    out.normal = normalize(normal_matrix * in.normal);
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Normalize interpolated normal
    let normal = normalize(in.normal);

    // Light direction (directional light)
    let light_dir = normalize(-u_lighting.light_direction);

    // Sample the diffuse texture
    let texture_color = textureSample(t_diffuse, s_diffuse, in.uv);
    let base_color = u_material.base_color.rgb * texture_color.rgb;

    // Lambert diffuse lighting
    let n_dot_l = max(dot(normal, light_dir), 0.0);
    let diffuse = u_lighting.light_color * u_lighting.light_intensity * n_dot_l;

    // Blinn-Phong specular (simple view direction from camera)
    let view_dir = normalize(-in.world_pos); // Assuming camera at origin for simplicity
    let half_dir = normalize(light_dir + view_dir);
    let n_dot_h = max(dot(normal, half_dir), 0.0);
    let shininess = 32.0;
    let specular_strength = 0.5;
    let specular = u_lighting.light_color * specular_strength * pow(n_dot_h, shininess);

    // Ambient lighting
    let ambient = u_lighting.light_color * u_lighting.ambient_intensity;

    // Final color: ambient + diffuse + specular
    let final_color = base_color * (ambient + diffuse) + specular;

    return vec4<f32>(final_color, u_material.base_color.a * texture_color.a);
}
