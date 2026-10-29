//! Walking an item's version graph in memory.
//!
//! An item's graph is small (its history, bounded by retention), so its edges
//! are loaded once and walked here instead of with recursive SQL.

use std::collections::{HashMap, HashSet};

use kryptopass_core::vault::VersionId;

/// Parents of each version that has any.
pub(crate) type Edges = HashMap<VersionId, Vec<VersionId>>;

/// Every version reachable from `starts` through parents, the starts included.
pub(crate) fn ancestors(
    edges: &Edges,
    starts: impl IntoIterator<Item = VersionId>,
) -> HashSet<VersionId> {
    let mut seen = HashSet::new();
    let mut stack: Vec<VersionId> = starts.into_iter().collect();
    while let Some(version) = stack.pop() {
        if seen.insert(version) {
            stack.extend(edges.get(&version).into_iter().flatten().copied());
        }
    }
    seen
}

/// Whether `version` is `ancestor` or descends from it.
pub(crate) fn descends_from(edges: &Edges, version: VersionId, ancestor: VersionId) -> bool {
    ancestors(edges, [version]).contains(&ancestor)
}

/// The best common ancestor of `a` and `b`: one that is not an ancestor of
/// another common ancestor. Criss-cross histories can have several; the
/// greatest id wins so every device picks the same one.
pub(crate) fn merge_base(edges: &Edges, a: VersionId, b: VersionId) -> Option<VersionId> {
    let of_a = ancestors(edges, [a]);
    let common: HashSet<VersionId> = ancestors(edges, [b])
        .into_iter()
        .filter(|version| of_a.contains(version))
        .collect();
    // Strict ancestors of any common ancestor are never the best one; one
    // walk from all their parents finds them all.
    let parents = common
        .iter()
        .flat_map(|version| edges.get(version).into_iter().flatten().copied());
    let dominated = ancestors(edges, parents);
    common
        .into_iter()
        .filter(|version| !dominated.contains(version))
        .max()
}
