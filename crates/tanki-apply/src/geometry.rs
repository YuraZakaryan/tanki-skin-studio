//! A geometry fingerprint of a 3DS model: the vertex and face counts of its meshes.
//!
//! The cache holds the current version of a model and a pack often ships an older export of the same
//! item, so the files differ byte for byte while the mesh is the same. The counts survive that, and a
//! model of a tank or a weapon has too many vertices to collide with another item by chance.

/// Mesh (vertices, faces) pairs of a model, sorted.
pub type Fingerprint = Vec<(u16, u16)>;

/// Models smaller than this (cubes, planes, shell stubs) say nothing about an item.
const MIN_VERTICES: u32 = 20;
const MAX_DEPTH: u8 = 8;

const MAIN: u16 = 0x4D4D;
const EDITOR: u16 = 0x3D3D;
const OBJECT: u16 = 0x4000;
const TRIMESH: u16 = 0x4100;
const VERTICES: u16 = 0x4110;
const FACES: u16 = 0x4120;

fn count(bytes: &[u8], at: usize) -> u16 {
    bytes
        .get(at..at + 2)
        .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
}

fn walk(
    bytes: &[u8],
    mut at: usize,
    end: usize,
    mesh: Option<usize>,
    meshes: &mut Vec<(u16, u16)>,
    depth: u8,
) {
    if depth > MAX_DEPTH {
        return;
    }
    while at + 6 <= end {
        let id = u16::from_le_bytes([bytes[at], bytes[at + 1]]);
        let len = u32::from_le_bytes([bytes[at + 2], bytes[at + 3], bytes[at + 4], bytes[at + 5]])
            as usize;
        if len < 6 {
            return;
        }
        let body = at + 6;
        let stop = at.saturating_add(len).min(end);
        match id {
            MAIN | EDITOR | TRIMESH => walk(bytes, body, stop, mesh, meshes, depth + 1),
            OBJECT => {
                // The object's name is a zero-terminated string before its sub-chunks.
                if let Some(name) = bytes[body..stop].iter().position(|c| *c == 0) {
                    meshes.push((0, 0));
                    let index = meshes.len() - 1;
                    walk(bytes, body + name + 1, stop, Some(index), meshes, depth + 1);
                }
            }
            VERTICES => {
                if let Some(index) = mesh {
                    meshes[index].0 = count(bytes, body);
                }
            }
            FACES => {
                if let Some(index) = mesh {
                    meshes[index].1 = count(bytes, body);
                }
            }
            _ => {}
        }
        at = at.saturating_add(len);
    }
}

/// The fingerprint of a 3DS file, or `None` when it is not one or is too small to name an item.
pub fn fingerprint(bytes: &[u8]) -> Option<Fingerprint> {
    if bytes.len() < 6 || u16::from_le_bytes([bytes[0], bytes[1]]) != MAIN {
        return None;
    }
    let mut meshes = Vec::new();
    walk(bytes, 0, bytes.len(), None, &mut meshes, 0);
    meshes.retain(|(vertices, _)| *vertices > 0);
    let total: u32 = meshes.iter().map(|(v, _)| u32::from(*v)).sum();
    if total < MIN_VERTICES {
        return None;
    }
    meshes.sort_unstable();
    Some(meshes)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn chunk(id: u16, body: &[u8]) -> Vec<u8> {
        let mut out = id.to_le_bytes().to_vec();
        out.extend(((body.len() + 6) as u32).to_le_bytes());
        out.extend(body);
        out
    }

    /// A 3DS file whose objects have the given (vertices, faces) counts.
    pub(crate) fn model(meshes: &[(u16, u16)]) -> Vec<u8> {
        let mut editor = Vec::new();
        for (index, (vertices, faces)) in meshes.iter().enumerate() {
            let mut mesh = chunk(VERTICES, &vertices.to_le_bytes());
            mesh.extend(chunk(FACES, &faces.to_le_bytes()));
            let mut object = format!("obj{index}\0").into_bytes();
            object.extend(chunk(TRIMESH, &mesh));
            editor.extend(chunk(OBJECT, &object));
        }
        chunk(MAIN, &chunk(EDITOR, &editor))
    }

    #[test]
    fn counts_survive_a_different_file() {
        let a = model(&[(120, 200), (40, 60)]);
        let mut b = model(&[(40, 60), (120, 200)]);
        b.extend([0u8; 16]);
        assert_eq!(fingerprint(&a), Some(vec![(40, 60), (120, 200)]));
        assert_eq!(fingerprint(&a), fingerprint(&b));
        assert_ne!(
            fingerprint(&a),
            fingerprint(&model(&[(120, 201), (40, 60)]))
        );
    }

    #[test]
    fn tiny_or_foreign_files_name_nothing() {
        assert_eq!(fingerprint(&model(&[(8, 12)])), None);
        assert_eq!(fingerprint(b"not a model"), None);
        assert_eq!(fingerprint(&[]), None);
        let mut cut = model(&[(120, 200)]);
        cut.truncate(cut.len() - 5);
        let _ = fingerprint(&cut);
    }
}
