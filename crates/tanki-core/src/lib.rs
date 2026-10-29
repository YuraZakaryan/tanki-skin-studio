//! Layer 3 facade. The logic lives in the lower crates; this keeps `tanki_core::cache::…` paths
//! stable for applications and hosts the end-to-end tests that cross every layer.

pub use tanki_apply::{apply, process, recognize};
pub use tanki_cache::{cache, targets};
pub use tanki_pack::{archive, pack};
pub use tanki_state::{paths, session, state};
