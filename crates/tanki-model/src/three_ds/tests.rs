// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use tanki_testkit::three_ds::{Mesh3ds, hull_3ds, model_3ds, model_counts};

use super::*;

#[test]
fn a_box_reads_back_with_its_names_uvs_and_smoothing() {
    let model = read(&hull_3ds()).unwrap();
    assert_eq!(model.meshes.len(), 2);
    let hull = model.mesh("HULL").unwrap();
    assert_eq!(hull.positions.len(), 24);
    assert_eq!(hull.uvs.len(), 24);
    assert_eq!(hull.faces.len(), 12);
    assert_eq!(hull.smoothing.len(), 12);
    assert_eq!(hull.positions[0], [-150.0, -250.0, 80.0]);
    assert_eq!(model.triangle_count(), 24);
}

#[test]
fn faces_naming_a_missing_or_repeated_vertex_are_dropped_with_their_smoothing() {
    let mut mesh = Mesh3ds::cuboid("hull", [0.0; 3], [1.0; 3]);
    mesh.faces.push([0, 1, 99]);
    mesh.faces.push([2, 2, 3]);
    mesh.smoothing.extend([7, 7]);
    let model = read(&model_3ds(&[mesh])).unwrap();
    assert_eq!(model.meshes[0].faces.len(), 12);
    assert_eq!(model.meshes[0].smoothing.len(), 12);
}

#[test]
fn a_mesh_with_a_non_finite_vertex_is_dropped() {
    let mut bad = Mesh3ds::cuboid("bad", [0.0; 3], [1.0; 3]);
    bad.positions[3] = [f32::NAN, 0.0, 0.0];
    let good = Mesh3ds::cuboid("good", [0.0; 3], [1.0; 3]);
    let model = read(&model_3ds(&[bad, good])).unwrap();
    assert_eq!(model.meshes.len(), 1);
    assert_eq!(model.meshes[0].name, "good");
}

#[test]
fn uvs_that_do_not_cover_every_vertex_are_ignored() {
    let mut mesh = Mesh3ds::cuboid("hull", [0.0; 3], [1.0; 3]);
    mesh.uvs.truncate(5);
    let model = read(&model_3ds(&[mesh])).unwrap();
    assert!(model.meshes[0].uvs.is_empty());
}

#[test]
fn other_files_and_empty_models_are_errors() {
    assert_eq!(read(b"not a model"), Err(ModelError::NotA3ds));
    assert_eq!(read(&[]), Err(ModelError::NotA3ds));
    assert_eq!(read(&model_3ds(&[])), Err(ModelError::NoMeshes));
    // Counts with no data behind them: nothing to draw.
    assert_eq!(
        read(&model_counts(&[(120, 200)])),
        Err(ModelError::NoMeshes)
    );
    assert_eq!(
        read(&vec![0u8; MAX_BYTES + 1]),
        Err(ModelError::TooLarge(MAX_BYTES + 1))
    );
}

#[test]
fn every_truncation_and_corruption_reads_without_panicking() {
    let whole = hull_3ds();
    for len in 0..whole.len() {
        let _ = read(&whole[..len]);
        let _ = fingerprint(&whole[..len]);
    }
    // A fixed pseudo-random walk of byte flips, so a failure is reproducible.
    let mut seed: u32 = 0x9E37_79B9;
    for _ in 0..2000 {
        let mut bytes = whole.clone();
        for _ in 0..4 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let at = seed as usize % bytes.len();
            bytes[at] = (seed >> 24) as u8;
        }
        if let Ok(model) = read(&bytes) {
            for mesh in &model.meshes {
                assert!(
                    mesh.faces
                        .iter()
                        .flatten()
                        .all(|i| usize::from(*i) < mesh.positions.len())
                );
            }
        }
        let _ = fingerprint(&bytes);
    }
}

#[test]
fn fingerprint_counts_survive_a_different_file() {
    let a = model_counts(&[(120, 200), (40, 60)]);
    let mut b = model_counts(&[(40, 60), (120, 200)]);
    b.extend([0u8; 16]);
    assert_eq!(fingerprint(&a), Some(vec![(40, 60), (120, 200)]));
    assert_eq!(fingerprint(&a), fingerprint(&b));
    assert_ne!(
        fingerprint(&a),
        fingerprint(&model_counts(&[(120, 201), (40, 60)]))
    );
}

#[test]
fn fingerprint_of_a_full_model_is_its_counts() {
    assert_eq!(fingerprint(&hull_3ds()), Some(vec![(24, 12), (24, 12)]));
}

#[test]
fn tiny_or_foreign_files_have_no_fingerprint() {
    assert_eq!(fingerprint(&model_counts(&[(8, 12)])), None);
    assert_eq!(fingerprint(b"not a model"), None);
    assert_eq!(fingerprint(&[]), None);
    let mut cut = model_counts(&[(120, 200)]);
    cut.truncate(cut.len() - 5);
    let _ = fingerprint(&cut);
}
