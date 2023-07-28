// This isn't the bevy's standard material, I just kept the name for some reason I don't remember.
struct StandardMaterial {
    base_color: vec4<f32>,
    emissive: vec4<f32>,
    perceptual_roughness: f32,
    metallic: f32,
    reflectance: f32,
    // 'flags' is a bit field indicating various options. u32 is 32 bits so we have up to 32 options.
    flags: u32,
    alpha_cutoff: f32,
    animation_frames: u32,
};

const STANDARD_MATERIAL_FLAGS_BASE_COLOR_TEXTURE_BIT: u32         = 1u;
const STANDARD_MATERIAL_FLAGS_EMISSIVE_TEXTURE_BIT: u32           = 2u;
const STANDARD_MATERIAL_FLAGS_METALLIC_ROUGHNESS_TEXTURE_BIT: u32 = 4u;
const STANDARD_MATERIAL_FLAGS_OCCLUSION_TEXTURE_BIT: u32          = 8u;
const STANDARD_MATERIAL_FLAGS_DOUBLE_SIDED_BIT: u32               = 16u;
const STANDARD_MATERIAL_FLAGS_UNLIT_BIT: u32                      = 32u;
const STANDARD_MATERIAL_FLAGS_TWO_COMPONENT_NORMAL_MAP: u32       = 64u;
const STANDARD_MATERIAL_FLAGS_FLIP_NORMAL_MAP_Y: u32              = 128u;
const STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT: u32                = 256u;
const STANDARD_MATERIAL_FLAGS_DEPTH_MAP_BIT: u32                  = 512u;
const STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS: u32       = 3758096384u; // (0b111u32 << 29)
const STANDARD_MATERIAL_FLAGS_ALPHA_MODE_OPAQUE: u32              = 0u;          // (0u32 << 29)
const STANDARD_MATERIAL_FLAGS_ALPHA_MODE_MASK: u32                = 536870912u;  // (1u32 << 29)
const STANDARD_MATERIAL_FLAGS_ALPHA_MODE_BLEND: u32               = 1073741824u; // (2u32 << 29)
const STANDARD_MATERIAL_FLAGS_ALPHA_MODE_PREMULTIPLIED: u32       = 1610612736u; // (3u32 << 29)
const STANDARD_MATERIAL_FLAGS_ALPHA_MODE_ADD: u32                 = 2147483648u; // (4u32 << 29)
const STANDARD_MATERIAL_FLAGS_ALPHA_MODE_MULTIPLY: u32            = 2684354560u; // (5u32 << 29)

fn standard_material_new() -> StandardMaterial {
    var material: StandardMaterial;

    // NOTE: Keep in-sync with src/pbr_material.rs!
    material.base_color = vec4<f32>(1.0, 1.0, 1.0, 1.0);
    material.emissive = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    material.perceptual_roughness = 0.089;
    material.metallic = 0.01;
    material.reflectance = 0.5;
    material.flags = STANDARD_MATERIAL_FLAGS_ALPHA_MODE_OPAQUE;
    material.alpha_cutoff = 0.5;

    return material;
}

#import bevy_pbr::mesh_view_bindings
#import bevy_pbr::mesh_bindings

#import bevy_pbr::utils
#import bevy_pbr::clustered_forward
#import bevy_pbr::lighting
#import bevy_pbr::pbr_ambient
#import bevy_pbr::shadows
#import bevy_pbr::fog
#import bevy_pbr::pbr_functions

#import bevy_pbr::prepass_utils

@group(1) @binding(0)
var<uniform> material: StandardMaterial;
@group(1) @binding(1)
var base_color_texture: texture_2d<f32>;
@group(1) @binding(2)
var base_color_sampler: sampler;
@group(1) @binding(3)
var emissive_texture: texture_2d<f32>;
@group(1) @binding(4)
var emissive_sampler: sampler;
@group(1) @binding(5)
var metallic_roughness_texture: texture_2d<f32>;
@group(1) @binding(6)
var metallic_roughness_sampler: sampler;
@group(1) @binding(7)
var occlusion_texture: texture_2d<f32>;
@group(1) @binding(8)
var occlusion_sampler: sampler;
@group(1) @binding(9)
var normal_map_texture: texture_2d<f32>;
@group(1) @binding(10)
var normal_map_sampler: sampler;
@group(1) @binding(11)
var texture_array: texture_2d_array<f32>;
@group(1) @binding(12)
var texture_array_sampler: sampler;

// for debug
fn get_light(light: u32) -> f32 {
    // TODO: This would be nice as a constant array, but dynamic indexing is not supported by naga.
    if light == 0u {
        return 0.03;
    } else if light == 1u {
        return 0.04;
    } else if light == 2u {
        return 0.05;
    } else if light == 3u {
        return 0.07;
    } else if light == 4u {
        return 0.09;
    } else if light == 5u {
        return 0.11;
    } else if light == 6u {
        return 0.135;
    } else if light == 7u {
        return 0.17;
    } else if light == 8u {
        return 0.21;
    } else if light == 9u {
        return 0.26;
    } else if light == 10u {
        return 0.38;
    } else if light == 11u {
        return 0.41;
    } else if light == 12u {
        return 0.51;
    } else if light == 13u {
        return 0.64;
    } else if light == 14u {
        return 0.8;
    } else if light == 15u {
        return 1.0;
    } else {
        return 0.0;
    }
}

@fragment
fn fragment(
    @builtin(front_facing) is_front: bool,
    @builtin(position) frag_coord: vec4<f32>,
    @builtin(sample_index) sample_index: u32,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) texture_index: i32,
#ifdef VERTEX_TANGENTS
    @location(4) world_tangent: vec4<f32>,
#endif
    @location(5) light: u32,
) -> @location(0) vec4<f32> {
    var output_color: vec4<f32> = material.base_color;

    // For some reason this refuses to take a u32 as the index
    let fps = 10.0;
    let texture_index: i32 = texture_index + i32(globals.time * fps) % i32(material.animation_frames);
    output_color = output_color * textureSample(texture_array, texture_array_sampler, uv, texture_index);

    let sunlight = (light >> 4u) & 0xFu;
    let artificial_light = light & 0xFu;
    let light = pow(0.8, f32(15u - max(sunlight, artificial_light)));
    //let light = get_light(sunlight);
    if sunlight >= artificial_light {
        output_color = vec4(output_color.rgb * clamp(light * lights.ambient_color.a, 0.03, 1.0), output_color.a);
    } else {
        output_color = vec4(output_color.rgb * light, output_color.a);
    }

    if abs(world_normal.z) == 1.0 {
        output_color = vec4(output_color.rgb * 0.8, output_color.a);
    } else if abs(world_normal.x) == 1.0 {
        output_color = vec4(output_color.rgb * 0.5, output_color.a);
    } else if world_normal.y == -1.0 {
        output_color = vec4(output_color.rgb * 0.3, output_color.a);
    }

    output_color = alpha_discard(material, output_color);

    // This is water depth, hard to figure out, don't know if useless, no delete.
    //if ((material.flags & STANDARD_MATERIAL_FLAGS_IS_WATER) != 0u) {
    //    let z_depth_ndc = prepass_depth(frag_coord, sample_index);
    //    let z_depth_buffer_view = view.projection[3][2] / z_depth_ndc;
    //    let z_fragment_view = view.projection[3][2] / frag_coord.z;
    //    let diff = z_fragment_view - z_depth_buffer_view;
    //    let alpha = min(exp(-diff * 0.08 - 1.0), 1.0);
    //    output_color.a = alpha;
    //}

//#ifdef VERTEX_COLORS
//    output_color = output_color * color;
//#endif
//#ifdef VERTEX_UVS
//    if ((material.flags & STANDARD_MATERIAL_FLAGS_BASE_COLOR_TEXTURE_BIT) != 0u) {
//        output_color = output_color * textureSample(base_color_texture, base_color_sampler, uv);
//    }
//#endif
//
//    // NOTE: Unlit bit not set means == 0 is true, so the true case is if lit
//    if ((material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) != 1u) {
//        // Prepare a 'processed' StandardMaterial by sampling all textures to resolve
//        // the material members
//        var pbr_input: PbrInput;
//
//        pbr_input.material.base_color = output_color;
//        pbr_input.material.reflectance = material.reflectance;
//        pbr_input.material.flags = material.flags;
//        pbr_input.material.alpha_cutoff = material.alpha_cutoff;
//
//        // TODO use .a for exposure compensation in HDR
//        var emissive: vec4<f32> = material.emissive;
//#ifdef VERTEX_UVS
//        if ((material.flags & STANDARD_MATERIAL_FLAGS_EMISSIVE_TEXTURE_BIT) != 0u) {
//            emissive = vec4<f32>(emissive.rgb * textureSample(emissive_texture, emissive_sampler, uv).rgb, 1.0);
//        }
//#endif
//        pbr_input.material.emissive = emissive;
//
//        var metallic: f32 = material.metallic;
//        var perceptual_roughness: f32 = material.perceptual_roughness;
//#ifdef VERTEX_UVS
//        if ((material.flags & STANDARD_MATERIAL_FLAGS_METALLIC_ROUGHNESS_TEXTURE_BIT) != 0u) {
//            let metallic_roughness = textureSample(metallic_roughness_texture, metallic_roughness_sampler, uv);
//            // Sampling from GLTF standard channels for now
//            metallic = metallic * metallic_roughness.b;
//            perceptual_roughness = perceptual_roughness * metallic_roughness.g;
//        }
//#endif
//        pbr_input.material.metallic = metallic;
//        pbr_input.material.perceptual_roughness = perceptual_roughness;
//
//        var occlusion: f32 = 1.0;
//#ifdef VERTEX_UVS
//        if ((material.flags & STANDARD_MATERIAL_FLAGS_OCCLUSION_TEXTURE_BIT) != 0u) {
//            occlusion = textureSample(occlusion_texture, occlusion_sampler, uv).r;
//        }
//#endif
//        pbr_input.frag_coord = frag_coord;
//        pbr_input.world_position = world_position;
//
//#ifdef LOAD_PREPASS_NORMALS
//        pbr_input.world_normal = prepass_normal(frag_coord, 0u);
//#else // LOAD_PREPASS_NORMALS
//        pbr_input.world_normal = prepare_world_normal(
//            world_normal,
//            (material.flags & STANDARD_MATERIAL_FLAGS_DOUBLE_SIDED_BIT) != 0u,
//            is_front,
//        );
//#endif // LOAD_PREPASS_NORMALS
//
//        pbr_input.is_orthographic = view.projection[3].w == 1.0;
//
//        pbr_input.N = apply_normal_mapping(
//            material.flags,
//            pbr_input.world_normal,
//#ifdef VERTEX_TANGENTS
//#ifdef STANDARDMATERIAL_NORMAL_MAP
//            world_tangent,
//#endif
//#endif
//#ifdef VERTEX_UVS
//            uv,
//#endif
//        );
//        pbr_input.V = calculate_view(world_position, pbr_input.is_orthographic);
//        pbr_input.occlusion = occlusion;
//
//        pbr_input.flags = mesh.flags;
//
//        output_color = pbr(pbr_input);
//    } else {
//        output_color = alpha_discard(material, output_color);
//    }
//
//    if (fog.mode != FOG_MODE_OFF && (material.flags & STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT) != 0u) {
//        output_color = apply_fog(output_color, world_position.xyz, view.world_position.xyz);
//    }
//
#ifdef TONEMAP_IN_SHADER
    //output_color = tone_mapping(output_color);
#ifdef DEBAND_DITHER
    var output_rgb = output_color.rgb;
    output_rgb = powsafe(output_rgb, 1.0 / 2.2);
    output_rgb = output_rgb + screen_space_dither(frag_coord.xy);
    // This conversion back to linear space is required because our output texture format is
    // SRGB; the GPU will assume our output is linear and will apply an SRGB conversion.
    output_rgb = powsafe(output_rgb, 2.2);
    output_color = vec4(output_rgb, output_color.a);
#endif
#endif
#ifdef PREMULTIPLY_ALPHA
    output_color = premultiply_alpha(material.flags, output_color);
#endif
    return output_color;
}
