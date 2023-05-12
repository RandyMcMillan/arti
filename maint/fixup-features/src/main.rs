//! A quick and dirty command-line tool to enforce certain properties about
//! Arti's Cargo.toml files.
//!
//!
//! Definitions.
//!    
//! - An **experimental** feature is one for which we do not provide semver guarantees.
//! - A **non-additive** feature is one whose behavior does something other than
//!   add functionality to its crate.  (For example, building statically or
//!   switching out a default is non-additive.)
//! - The **meta** features are `default`, `full`, `experimental`,
//!   `__is_nonadditive`, and `__is_experimental`.
//! - The **toplevel** features are `default`, `full`, and `experimental`.
//! - A feature A "is reachable from" some feature B if there is a nonempty path from A
//!   to B in the feature graph.
//! - A feature A "directly depends on" some feature B if there is an edge from
//!   A to B in the feature graph.  We also say that feature B "is listed in"
//!   feature A.
//!
//! The properties that we want to enforce are:
//!
//! 1. Every crate has a "full" feature.
//! 2. For every crate within Arti, if we depend on that crate, our "full"
//!    includes that crate's "full".
//! 3. Every feature listed in `experimental` depends on `__is_experimental`.
//!    Every feature that depends on `__is_experimental` is reachable from `experimental`.
//!    Call such features "experimental" features.
//! 4. Call a feature "non-additive" if and only if it depends directly on `__is_nonadditive`.
//!    Every non-meta feature we declare is reachable from "full" or "experimental",
//!    or it is non-additive.
//! 5. Every feature reachable from `default` is reachable from `full`.
//! 6. No non-additive feature is reachable from `full` or `experimental`.
//! 7. No experimental is reachable from `full`.
//!
//!XXXX EDIT TO BECOME ACCURATE This tool can edit Cargo.toml files to enforce the rules 1 and 2
//! automatically.  For rule 3, it can annotate any offending features with
//! comments complaining about how they need to be included in one of the
//! top-level features.
//!
//! # To use:
//!
//! Run this tool with the top-level Cargo.toml as an argument.
//!
//! # Limitations
//!
//! This is not very efficient, and is not trying to be.

mod changes;
mod graph;

use anyhow::{anyhow, Context, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use toml_edit::{Document, Item, Table, Value};

use changes::{Change, Changes};

/// A warning we return from our linter.
///
/// It's a newtype so I don't confuse it with other strings.
#[derive(Debug, Clone)]
struct Warning(String);

/// A dependency from a crate.  
///
/// All we care about is the dependency's name, and whether it is optional.
#[derive(Debug, Clone)]
struct Dependency {
    name: String,
    optional: bool,
}

/// Stored information about a crate.
#[derive(Debug, Clone)]
struct Crate {
    /// name of the crate
    name: String,
    /// path to the crate's Cargo.toml
    toml_file: PathBuf,
    /// Parsed and manipulated copy of Cargo.toml
    toml_doc: Document,
    /// Parsed and un-manipulated copy of Cargo.toml.
    toml_doc_orig: Document,
}

/// Given a `[dependencies]` table from a Cargo.toml, find all of the
/// dependencies that are also part of arti.
///
/// We do this by looking for ones that have `path` set.
fn arti_dependencies(dependencies: &Table) -> Vec<Dependency> {
    let mut deps = Vec::new();

    for (depname, info) in dependencies {
        let table = match info {
            // Cloning is "inefficient", but we don't care.
            Item::Value(Value::InlineTable(info)) => info.clone().into_table(),
            Item::Table(info) => info.clone(),
            _ => continue, // Not part of arti.
        };
        if !table.contains_key("path") {
            continue; // Not part of arti.
        }
        let optional = table
            .get("optional")
            .and_then(Item::as_value)
            .and_then(Value::as_bool)
            .unwrap_or(false);

        deps.push(Dependency {
            name: depname.to_string(),
            optional,
        });
    }

    deps
}

/// A complaint that we add to features which are not reachable according to
/// rule 3.
const COMPLAINT: &str = "# XX\x58X Add this to a top-level feature!\n";

impl Crate {
    /// Try to read a crate's Cargo.toml from a given filename.
    fn load(p: impl AsRef<Path>) -> Result<Self> {
        let toml_file = p.as_ref().to_owned();
        let s = std::fs::read_to_string(&toml_file)?;
        let toml_doc = s.parse::<Document>()?;
        let toml_doc_orig = toml_doc.clone();
        let name = toml_doc["package"]["name"]
            .as_str()
            .ok_or_else(|| anyhow!("package.name was not a string"))?
            .to_string();
        Ok(Crate {
            name,
            toml_file,
            toml_doc,
            toml_doc_orig,
        })
    }

    /// Try to fix all the issues we find with a Cargo.toml.  Return a list of warnings.
    fn fix(&mut self) -> Result<Vec<Warning>> {
        let mut warnings = Vec::new();
        let mut w = |s| warnings.push(Warning(s));
        let dependencies = self
            .toml_doc
            .entry("dependencies")
            .or_insert_with(|| Item::Table(Table::new()));
        let dependencies = arti_dependencies(
            dependencies
                .as_table()
                .ok_or_else(|| anyhow!("dependencies was not a table"))?,
        );
        let features = self
            .toml_doc
            .entry("features")
            .or_insert_with(|| Item::Table(Table::new()))
            .as_table_mut()
            .ok_or_else(|| anyhow!("Features was not table"))?;
        let graph = graph::FeatureGraph::from_features_table(features)?;
        let mut changes = Changes::default();

        // Enforce rule 1.  (There is a "Full" feature.)
        if !graph.contains_feature("full") {
            w("full feature does not exist. Adding.".to_string());
            changes.push(Change::AddFeature("full".to_string()));
        }

        // Enforce rule 2. (for every arti crate that we depend on, our 'full' should include that crate's full.
        for dep in dependencies.iter() {
            let wanted = if dep.optional {
                format!("{}?/full", dep.name)
            } else {
                format!("{}/full", dep.name)
            };

            if !graph.contains_edge("full", wanted.as_str()) {
                w(format!("full should contain {}. Fixing.", wanted));
                changes.push(Change::AddEdge("full".to_string(), wanted));
            }
        }

        // Enforce rule 3 (relationship between "experimental" and "__is_experimental")
        {
            let in_experimental: HashSet<_> = graph.edges_from("experimental").collect();
            let is_experimental: HashSet<_> = graph.edges_to("__is_experimental").collect();
            let reachable_from_experimental: HashSet<_> =
                graph.all_reachable_from("experimental").collect();

            // Every feature listed in `experimental` depends on `__is_experimental`.
            for f in in_experimental.difference(&is_experimental) {
                w(format!("{f} should depend on __is_experimental. Fixing."));
                changes.push(Change::AddEdge(f.clone(), "__is_experimenal".into()));
            }
            // Every feature that depends on `__is_experimental` is reachable from `experimental`.
            for f in is_experimental.difference(&reachable_from_experimental) {
                w(format!("{f} is marked as __is_experimental, but is not reachable from experimental. Fixing."));
                changes.push(Change::AddEdge("experimental".into(), f.clone()))
            }
        };

        let all_features: HashSet<_> = graph.all_features().collect();
        let full: HashSet<_> = graph.all_reachable_from("full").collect();
        let experimental: HashSet<_> = graph.all_reachable_from("experimental").collect();
        let nonadditive: HashSet<_> = graph.all_reachable_from("__nonadditive").collect();
        let reachable_from_toplevel: HashSet<_> = [&full, &experimental, &nonadditive]
            .iter()
            .flat_map(|s| s.iter())
            .cloned()
            .collect();

        // Enforce rule 4: No feature we declare may be reachable from two of full,
        // experimental, and __nonadditive.
        for item in experimental.intersection(&full) {
            w(format!("{item} reachable from both full and experimental"));
        }
        for item in nonadditive.intersection(&full) {
            w(format!("{item} reachable from both full and nonadditive"));
        }
        for item in nonadditive.intersection(&experimental) {
            w(format!(
                "{item} reachable from both experimental and nonadditive"
            ));
        }

        // Enforce rule 3: Every feature we declare must be reachable from full,
        // experimental, or __nonadditive, except for those
        // top-level features, and "default".
        for feat in all_features.difference(&reachable_from_toplevel) {
            if ["full", "default", "experimental", "__nonadditive"].contains(&feat.as_ref()) {
                continue;
            }
            w(format!(
                "{feat} not reachable from full, experimental, or __nonadditive. Marking."
            ));

            changes.push(Change::Annotate(feat.clone(), COMPLAINT.to_string()));
        }

        changes.apply(features)?;

        Ok(warnings)
    }

    /// If we made changes to this crate's cargo.toml, flush it to disk.
    fn save_if_changed(&self) -> Result<()> {
        let old_text = self.toml_doc_orig.to_string();
        let new_text = self.toml_doc.to_string();
        if new_text != old_text {
            println!("{} changed. Replacing.", self.name);
            let tmpname = self.toml_file.with_extension("toml.tmp");
            std::fs::write(&tmpname, new_text.as_str())?;
            std::fs::rename(&tmpname, &self.toml_file)?;
        }
        Ok(())
    }
}

/// Look at a toplevel Cargo.toml and find all of the paths in workplace.members
fn list_crate_paths(toplevel: impl AsRef<Path>) -> Result<Vec<String>> {
    let s = std::fs::read_to_string(toplevel.as_ref())?;
    let toml_doc = s.parse::<Document>()?;
    Ok(toml_doc["workspace"]["members"]
        .as_array()
        .ok_or_else(|| anyhow!("workplace.members is not an array!?"))?
        .iter()
        .map(|v| {
            v.as_str()
                .expect("Some member of workplace.members is not a string!?")
                .to_owned()
        })
        .collect())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 1 {
        println!("We expect a single argument: The top-level Cargo.toml file.");
        return Ok(());
    }
    let toplevel_toml_file = PathBuf::from(&args[1]);
    let toplevel_dir = toplevel_toml_file
        .parent()
        .expect("How is your Cargo.toml file `/`?")
        .to_path_buf();
    let mut crates = Vec::new();
    for p in list_crate_paths(&toplevel_toml_file)? {
        let mut crate_toml_path = toplevel_dir.clone();
        crate_toml_path.push(p);
        crate_toml_path.push("Cargo.toml");
        crates.push(
            Crate::load(&crate_toml_path).with_context(|| format!("In {crate_toml_path:?}"))?,
        );
    }

    for cr in crates.iter_mut() {
        for w in cr.fix().with_context(|| format!("In {}", cr.name))? {
            println!("{}: {}", cr.name, w.0);
        }
        cr.save_if_changed()?;
    }

    Ok(())
}
