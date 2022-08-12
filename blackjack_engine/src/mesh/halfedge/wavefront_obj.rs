// Copyright (C) 2022 setzer22 and contributors
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use slotmap::SecondaryMap;
use std::{
    fs::File,
    io::{BufReader, BufWriter, Write},
    path::PathBuf,
};
use wavefront_rs::obj::{
    self,
    entity::{Entity, FaceVertex},
};

use crate::prelude::*;

impl HalfEdgeMesh {
    pub fn to_wavefront_obj(&self, path: impl Into<PathBuf>) -> Result<()> {
        let mut writer = BufWriter::new(File::create(path.into())?);

        // We need to store the mapping between vertex ids and indices in the
        // generated OBJ
        // NOTE: OBJ Wavefront indices start at 1
        let mut imap = SecondaryMap::<VertexId, i32>::new();

        obj::format_writer::FormatWriter::write(
            &mut writer,
            &Entity::Comment {
                content: "Generated by Blackjack: https://github.com/setzer22/blackjack".into(),
            },
        );
        writeln!(writer)?;

        let conn = self.read_connectivity();

        for (idx, (v_id, _, pos)) in conn
            .iter_vertices_with_channel(&self.read_positions())
            .enumerate()
        {
            imap.insert(v_id, (idx + 1) as i32);
            obj::format_writer::FormatWriter::write(
                &mut writer,
                &Entity::Vertex {
                    x: pos.x as f64,
                    y: pos.y as f64,
                    z: pos.z as f64,
                    w: None,
                },
            );
            writeln!(writer)?;
        }

        let mut has_normals = false;
        if self.gen_config.smooth_normals {
            if let Some(v_normals_ch) = self.read_vertex_normals() {
                has_normals = true;
                for (v, _) in conn.iter_vertices() {
                    let normal = v_normals_ch[v];
                    obj::format_writer::FormatWriter::write(
                        &mut writer,
                        &Entity::VertexNormal {
                            x: normal.x as f64,
                            y: normal.y as f64,
                            z: normal.z as f64,
                        },
                    );
                    writeln!(writer)?;
                }
            }
        } else {
            // TODO has_normals = true;
            println!("TODO: Exporting per-face normals is not yet implemented.")
        }

        // Since UVs are stored in halfedges, we need the same mapping as `imap`
        // above, but for halfedges instead.
        let mut h_imap = SecondaryMap::<HalfEdgeId, i32>::new();
        let mut has_uvs = false;
        if let Some(uvs_ch) = self.read_uvs() {
            has_uvs = true;
            for (idx, (h, _)) in conn.iter_halfedges().enumerate() {
                h_imap.insert(h, (idx + 1) as i32);
                let uv = uvs_ch[h];
                obj::format_writer::FormatWriter::write(
                    &mut writer,
                    &Entity::VertexTexture {
                        u: uv.x as f64,
                        v: Some(uv.y as f64),
                        w: None,
                    },
                );
                writeln!(writer)?;
            }
        }

        for (face_id, _) in conn.iter_faces() {
            let vertices = conn
                .face_vertices(face_id)
                .iter()
                .zip(conn.face_edges(face_id).iter())
                .map(|(v_id, h_id)| FaceVertex {
                    vertex: imap[*v_id] as i64,
                    // TODO: For now we rely on emitting one normal per vertex.
                    // Sometimes there might be less, when we implement flat
                    // shaded normals.
                    normal: if has_normals {
                        Some(imap[*v_id] as i64)
                    } else {
                        None
                    },
                    texture: if has_uvs {
                        Some(h_imap[*h_id] as i64)
                    } else {
                        None
                    },
                })
                .collect();
            obj::format_writer::FormatWriter::write(&mut writer, &Entity::Face { vertices });
            writeln!(writer)?;
        }

        Ok(())
    }

    pub fn from_wavefront_obj(path: PathBuf) -> Result<HalfEdgeMesh> {
        let mut reader = BufReader::new(File::open(path)?);
        let mut positions = vec![];
        let mut polygons = vec![];
        obj::read_lexer::ReadLexer::read_to_end(&mut reader, |entity| match entity {
            Entity::Vertex { x, y, z, w: _w } => {
                positions.push(Vec3::new(x as f32, y as f32, z as f32));
            }
            Entity::Face { vertices } => {
                // NOTE: OBJ Wavefront indices start at 1
                let polygon: SVec<usize> =
                    vertices.iter().map(|v| (v.vertex - 1) as usize).collect();
                polygons.push(polygon);
            }
            _ => {}
        })?;
        halfedge::HalfEdgeMesh::build_from_polygons(&positions, &polygons)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn test_load_obj() {
        HalfEdgeMesh::from_wavefront_obj("./assets/debug/arrow.obj".into())
            .unwrap()
            .to_wavefront_obj("/tmp/wat.obj")
            .unwrap();
    }
}
