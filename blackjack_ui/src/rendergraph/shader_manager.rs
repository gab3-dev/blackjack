// Copyright (C) 2022 setzer22 and contributors
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::HashMap;

use wgpu::{BlendState, ColorTargetState, FragmentState, VertexBufferLayout, VertexState};

pub struct Shader {
    pub fs_entry_point: String,
    pub vs_entry_point: String,
    pub module: wgpu::ShaderModule,
    pub color_targets: Vec<Option<ColorTargetState>>,
}

impl Shader {
    pub fn to_vertex_state<'a>(&'a self, buffers: &'a [VertexBufferLayout]) -> VertexState {
        VertexState {
            module: &self.module,
            entry_point: &self.vs_entry_point,
            buffers,
        }
    }

    pub fn get_fragment_state(&self) -> FragmentState {
        FragmentState {
            module: &self.module,
            entry_point: &self.fs_entry_point,
            /*targets: &[Some(ColorTargetState {
                format: wgpu::TextureFormat::Rgba16Float,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],*/
            targets: &self.color_targets,
        }
    }
}

pub struct ShaderManager {
    pub shaders: HashMap<String, Shader>,
}

impl ShaderManager {
    pub fn new(device: &wgpu::Device) -> Self {
        let mut shaders = HashMap::new();

        let mut context = glsl_include::Context::new();
        let context = context
            .include("utils.wgsl", include_str!("utils.wgsl"))
            .include("rend3_common.wgsl", include_str!("rend3_common.wgsl"))
            .include("rend3_vertex.wgsl", include_str!("rend3_vertex.wgsl"))
            .include("rend3_object.wgsl", include_str!("rend3_object.wgsl"))
            .include("rend3_uniforms.wgsl", include_str!("rend3_uniforms.wgsl"));

        macro_rules! def_shader {
            ($name:expr, $src:expr, opaque) => {
                def_shader!(
                    $name,
                    $src,
                    custom,
                    vec![Some(ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL
                    })]
                )
            };
            ($name:expr, $src:expr, alpha_blend) => {
                def_shader!(
                    $name,
                    $src,
                    custom,
                    vec![Some(ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: Some(BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL
                    })]
                )
            };
            ($name:expr, $src:expr, custom, $targets:expr) => {
                shaders.insert(
                    $name.to_string(),
                    Shader {
                        fs_entry_point: "fs_main".into(),
                        vs_entry_point: "vs_main".into(),
                        module: device.create_shader_module(wgpu::ShaderModuleDescriptor {
                            label: Some($name),
                            source: wgpu::ShaderSource::Wgsl(
                                context
                                    .expand(include_str!($src))
                                    .expect("Shader preprocessor")
                                    .into(),
                            ),
                        }),
                        color_targets: $targets,
                    },
                );
            };
        }

        // A bit unconventional, but shaders define their own color targets.
        // Most shaders will draw to a single Rgba16Float color buffer, either
        // in opaque mode or using alpha blending.
        //
        // But for some shaders, the targets will be entirely different.
        def_shader!("edge_wireframe_draw", "edge_wireframe_draw.wgsl", opaque);
        def_shader!("point_cloud_draw", "point_cloud_draw.wgsl", opaque);
        def_shader!("face_draw", "face_draw.wgsl", opaque);
        def_shader!("face_overlay_draw", "face_overlay_draw.wgsl", alpha_blend);

        def_shader!(
            "face_id_draw",
            "face_id_draw.wgsl",
            custom,
            vec![
                // First, the id channel, as u32
                Some(ColorTargetState {
                    format: wgpu::TextureFormat::R32Uint,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                }),
                // Then, a regular color channel, for the debug buffer
                Some(ColorTargetState {
                    format: wgpu::TextureFormat::R32Uint,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                }),
            ]
        );

        Self { shaders }
    }

    pub fn get(&self, shader_name: &str) -> &Shader {
        self.shaders.get(shader_name).unwrap()
    }
}
