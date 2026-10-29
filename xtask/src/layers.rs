//! Dependency layering rules, enforced by `cargo xtask layers` (same idea as PdfCraft's xtask).
//!
//! The rule engine works on a small model that does not depend on cargo, so it can be unit
//! tested; `from_metadata` builds that model from `cargo metadata`.

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// Regular layered crate: may depend only on workspace crates of a lower layer.
    Layer(u8),
    /// Test fixtures: a dev-dependency only, and depends on no workspace crate itself.
    Testkit,
    /// Binaries and build tooling: exempt.
    Exempt,
}

/// Every workspace crate and its layer. A new crate must be classified here.
pub const TABLE: &[(&str, Class)] = &[
    ("tanki-ui", Class::Layer(0)),
    ("tanki-i18n", Class::Layer(0)),
    ("tanki-cache", Class::Layer(1)),
    ("tanki-pack", Class::Layer(1)),
    ("tanki-apply", Class::Layer(2)),
    ("tanki-state", Class::Layer(2)),
    ("tanki-core", Class::Layer(3)),
    ("tanki-testkit", Class::Testkit),
    ("tanki-studio", Class::Exempt),
    ("tanki-cli", Class::Exempt),
    ("xtask", Class::Exempt),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dep {
    pub name: String,
    pub workspace: bool,
    pub dev: bool,
}

#[derive(Debug, Clone)]
pub struct CrateInfo {
    pub name: String,
    pub deps: Vec<Dep>,
}

pub fn classify(name: &str) -> Option<Class> {
    TABLE.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

pub fn describe(class: Class) -> String {
    match class {
        Class::Layer(n) => format!("layer {n}"),
        Class::Testkit => "testkit".into(),
        Class::Exempt => "binary/tool".into(),
    }
}

/// All violations of the layering rules; empty means the graph is clean.
pub fn check(crates: &[CrateInfo]) -> Vec<String> {
    let mut violations = Vec::new();
    for krate in crates {
        let Some(class) = classify(&krate.name) else {
            violations.push(format!(
                "{}: not classified; add it to layers::TABLE",
                krate.name
            ));
            continue;
        };
        for dep in krate.deps.iter().filter(|d| d.workspace) {
            let Some(dep_class) = classify(&dep.name) else {
                violations.push(format!(
                    "{} -> {}: dependency is not classified",
                    krate.name, dep.name
                ));
                continue;
            };
            match (class, dep_class) {
                (Class::Exempt, _) => {}
                (_, Class::Testkit) if dep.dev => {}
                (_, Class::Testkit) => violations.push(format!(
                    "{} -> {}: the testkit may only be a dev-dependency",
                    krate.name, dep.name
                )),
                (Class::Testkit, _) => violations.push(format!(
                    "{} -> {}: the testkit must not depend on workspace crates",
                    krate.name, dep.name
                )),
                (_, Class::Exempt) => violations.push(format!(
                    "{} -> {}: nothing may depend on a binary or tool",
                    krate.name, dep.name
                )),
                (Class::Layer(from), Class::Layer(to)) if to >= from => violations.push(format!(
                    "{} (layer {from}) -> {} (layer {to}): a crate may only depend on lower layers",
                    krate.name, dep.name
                )),
                (Class::Layer(_), Class::Layer(_)) => {}
            }
        }
    }
    violations
}

/// Build the model from `cargo metadata --no-deps` output. Path dependencies are the
/// workspace's own crates; every dependency here is a path dependency or an external one.
pub fn from_metadata(metadata: &Value) -> Result<Vec<CrateInfo>, String> {
    let packages = metadata["packages"]
        .as_array()
        .ok_or("metadata has no packages")?;
    packages
        .iter()
        .map(|p| {
            let name = p["name"]
                .as_str()
                .ok_or("package without a name")?
                .to_owned();
            let deps = p["dependencies"]
                .as_array()
                .ok_or("package without dependencies")?
                .iter()
                .map(|d| Dep {
                    name: d["name"].as_str().unwrap_or_default().to_owned(),
                    workspace: d["path"].is_string(),
                    dev: d["kind"].as_str() == Some("dev"),
                })
                .collect();
            Ok(CrateInfo { name, deps })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn krate(name: &str, deps: &[(&str, bool)]) -> CrateInfo {
        CrateInfo {
            name: name.into(),
            deps: deps
                .iter()
                .map(|(n, dev)| Dep {
                    name: (*n).into(),
                    workspace: true,
                    dev: *dev,
                })
                .collect(),
        }
    }

    #[test]
    fn the_real_layout_is_clean() {
        let crates = [
            krate("tanki-cache", &[]),
            krate("tanki-pack", &[]),
            krate(
                "tanki-apply",
                &[("tanki-cache", false), ("tanki-pack", false)],
            ),
            krate(
                "tanki-state",
                &[("tanki-cache", false), ("tanki-pack", false)],
            ),
            krate(
                "tanki-core",
                &[
                    ("tanki-apply", false),
                    ("tanki-state", false),
                    ("tanki-testkit", true),
                ],
            ),
            krate("tanki-testkit", &[]),
            krate(
                "tanki-studio",
                &[("tanki-core", false), ("tanki-ui", false)],
            ),
            krate("tanki-ui", &[]),
        ];
        assert_eq!(check(&crates), Vec::<String>::new());
    }

    #[test]
    fn upward_and_sideways_dependencies_are_rejected() {
        let up = check(&[krate("tanki-cache", &[("tanki-apply", false)])]);
        assert!(up[0].contains("lower layers"), "{up:?}");
        let sideways = check(&[krate("tanki-apply", &[("tanki-state", false)])]);
        assert!(sideways[0].contains("lower layers"), "{sideways:?}");
    }

    #[test]
    fn the_ui_kit_stays_independent_of_the_domain() {
        let bad = check(&[krate("tanki-ui", &[("tanki-cache", false)])]);
        assert!(bad[0].contains("lower layers"), "{bad:?}");
    }

    #[test]
    fn testkit_and_binaries_are_constrained() {
        assert!(
            check(&[krate("tanki-core", &[("tanki-testkit", false)])][..])[0]
                .contains("dev-dependency")
        );
        assert!(
            check(&[krate("tanki-testkit", &[("tanki-cache", false)])])[0]
                .contains("must not depend")
        );
        assert!(
            check(&[krate("tanki-cache", &[("tanki-cli", false)])])[0].contains("binary or tool")
        );
    }

    #[test]
    fn unknown_crates_must_be_classified() {
        let bad = check(&[krate("tanki-new", &[])]);
        assert!(bad[0].contains("not classified"), "{bad:?}");
    }

    #[test]
    fn metadata_marks_path_dependencies_as_workspace_crates() {
        let json = serde_json::json!({"packages": [{"name": "tanki-apply", "dependencies": [
            {"name": "tanki-cache", "path": "/x/tanki-cache", "kind": null},
            {"name": "serde", "path": null, "kind": null},
            {"name": "tanki-testkit", "path": "/x/tk", "kind": "dev"}]}]});
        let crates = from_metadata(&json).unwrap();
        let deps = &crates[0].deps;
        assert!(deps[0].workspace && !deps[0].dev);
        assert!(!deps[1].workspace);
        assert!(deps[2].workspace && deps[2].dev);
    }
}
