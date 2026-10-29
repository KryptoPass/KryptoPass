//! Walking a parent map, such as the folder tree.

use std::collections::HashMap;
use std::hash::Hash;

/// Where a node sits relative to a would-be ancestor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ancestry {
    /// The node is the ancestor or lies below it.
    Within,
    /// The node is elsewhere in the tree.
    Outside,
    /// The walk met a node missing from the map, or a cycle. Stored data is
    /// corrupt; callers refuse the operation instead of guessing.
    Broken,
}

/// Walks from `node` up through `parents` looking for `ancestor`. Each step
/// visits a new node at most once, so a corrupt cycle ends the walk.
pub fn ancestry<K: Copy + Eq + Hash>(
    parents: &HashMap<K, Option<K>>,
    node: K,
    ancestor: K,
) -> Ancestry {
    let mut current = node;
    for _ in 0..=parents.len() {
        if current == ancestor {
            return Ancestry::Within;
        }
        match parents.get(&current) {
            None => return Ancestry::Broken,
            Some(None) => return Ancestry::Outside,
            Some(Some(parent)) => current = *parent,
        }
    }
    Ancestry::Broken
}
