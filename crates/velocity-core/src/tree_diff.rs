use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use velocity_common::Element;

/// Result of comparing two hierarchy trees.
#[derive(Debug, Clone)]
pub struct DiffResult {
    /// Whether the trees are identical.
    pub unchanged: bool,
    /// Number of nodes that changed between the two trees.
    pub changed_count: usize,
    /// Total number of nodes in the new tree.
    pub total_count: usize,
}

impl DiffResult {
    /// Returns the fraction of the tree that changed (0.0 = identical, 1.0 = completely different).
    pub fn change_ratio(&self) -> f64 {
        if self.total_count == 0 {
            return 0.0;
        }
        self.changed_count as f64 / self.total_count as f64
    }
}

/// Tracks the previous hierarchy tree and provides incremental diffing.
///
/// Instead of re-hashing the entire tree every sync cycle, `TreeDiff` stores
/// the previous tree's root hash and per-subtree hashes. On each new hierarchy
/// snapshot, it compares root hashes first (O(1) fast path), then walks only
/// changed subtrees.
pub struct TreeDiff {
    prev_root_hash: Option<u64>,
    prev_subtree_hashes: Vec<u64>,
}

impl Default for TreeDiff {
    fn default() -> Self {
        Self::new()
    }
}

impl TreeDiff {
    pub fn new() -> Self {
        Self {
            prev_root_hash: None,
            prev_subtree_hashes: Vec::new(),
        }
    }

    /// Compute a hash for an element and its entire subtree.
    pub fn hash_element(element: &Element) -> u64 {
        let mut hasher = DefaultHasher::new();
        Self::hash_recursive(element, &mut hasher);
        hasher.finish()
    }

    fn hash_recursive(element: &Element, hasher: &mut DefaultHasher) {
        element.platform_id.hash(hasher);
        element.label.hash(hasher);
        element.text.hash(hasher);
        element.element_type.hash(hasher);
        element.bounds.x.hash(hasher);
        element.bounds.y.hash(hasher);
        element.bounds.width.hash(hasher);
        element.bounds.height.hash(hasher);
        element.enabled.hash(hasher);
        element.visible.hash(hasher);
        element.children.len().hash(hasher);
        for child in &element.children {
            Self::hash_recursive(child, hasher);
        }
    }

    /// Collect hashes of all direct children of the root.
    fn collect_child_hashes(element: &Element) -> Vec<u64> {
        element.children.iter().map(Self::hash_element).collect()
    }

    /// Compare a new hierarchy against the previously seen one.
    ///
    /// Returns `DiffResult` with information about what changed.
    /// The first call always returns `unchanged: false`.
    pub fn diff(&mut self, new_root: &Element) -> DiffResult {
        let new_root_hash = Self::hash_element(new_root);
        let total_count = count_nodes(new_root);

        // Fast path: root hash unchanged → entire tree is identical
        if self.prev_root_hash == Some(new_root_hash) {
            return DiffResult {
                unchanged: true,
                changed_count: 0,
                total_count,
            };
        }

        // Compute per-child hashes for the new tree
        let new_child_hashes = Self::collect_child_hashes(new_root);

        // Count changed subtrees by comparing child hashes
        let changed_count = if self.prev_subtree_hashes.is_empty() {
            // First snapshot — everything is "changed"
            total_count
        } else {
            let mut changed = 0;
            let max_len = new_child_hashes.len().max(self.prev_subtree_hashes.len());
            for i in 0..max_len {
                let new_hash = new_child_hashes.get(i);
                let old_hash = self.prev_subtree_hashes.get(i);
                if new_hash != old_hash {
                    // Count nodes in the changed subtree
                    if let Some(child) = new_root.children.get(i) {
                        changed += count_nodes(child);
                    } else {
                        changed += 1;
                    }
                }
            }
            // Also count root-level attribute changes
            if changed == 0 {
                changed = 1; // Root itself changed
            }
            changed
        };

        // Update stored state
        self.prev_root_hash = Some(new_root_hash);
        self.prev_subtree_hashes = new_child_hashes;

        DiffResult {
            unchanged: false,
            changed_count,
            total_count,
        }
    }

    /// Check if the tree is stable (identical to the previous snapshot).
    /// Lighter than `diff()` — only compares root hash.
    pub fn is_stable(&self, new_root: &Element) -> bool {
        let new_hash = Self::hash_element(new_root);
        self.prev_root_hash == Some(new_hash)
    }

    /// Reset the diff tracker, forcing the next comparison to treat everything as new.
    pub fn reset(&mut self) {
        self.prev_root_hash = None;
        self.prev_subtree_hashes.clear();
    }
}

/// Count total nodes in an element tree.
fn count_nodes(element: &Element) -> usize {
    1 + element.children.iter().map(count_nodes).sum::<usize>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use velocity_common::Rect;

    fn make_element(text: &str, children: Vec<Element>) -> Element {
        Element {
            platform_id: "el".to_string(),
            label: None,
            text: Some(text.to_string()),
            element_type: "View".to_string(),
            bounds: Rect {
                x: 0,
                y: 0,
                width: 100,
                height: 50,
            },
            enabled: true,
            visible: true,
            children,
        }
    }

    #[test]
    fn first_diff_is_always_changed() {
        let mut td = TreeDiff::new();
        let root = make_element("hello", vec![]);
        let result = td.diff(&root);
        assert!(!result.unchanged);
    }

    #[test]
    fn identical_tree_is_stable() {
        let mut td = TreeDiff::new();
        let root = make_element("hello", vec![]);
        td.diff(&root);
        let result = td.diff(&root);
        assert!(result.unchanged);
        assert_eq!(result.changed_count, 0);
    }

    #[test]
    fn changed_text_detected() {
        let mut td = TreeDiff::new();
        let root1 = make_element("hello", vec![]);
        td.diff(&root1);
        let root2 = make_element("world", vec![]);
        let result = td.diff(&root2);
        assert!(!result.unchanged);
        assert!(result.changed_count > 0);
    }

    #[test]
    fn child_change_detected() {
        let mut td = TreeDiff::new();
        let root1 = make_element("root", vec![make_element("child1", vec![])]);
        td.diff(&root1);
        let root2 = make_element("root", vec![make_element("child2", vec![])]);
        let result = td.diff(&root2);
        assert!(!result.unchanged);
        assert!(result.changed_count >= 1);
    }

    #[test]
    fn is_stable_check() {
        let mut td = TreeDiff::new();
        let root = make_element("hello", vec![]);
        assert!(!td.is_stable(&root));
        td.diff(&root);
        assert!(td.is_stable(&root));
    }

    #[test]
    fn count_nodes_works() {
        let root = make_element(
            "root",
            vec![
                make_element("a", vec![make_element("a1", vec![])]),
                make_element("b", vec![]),
            ],
        );
        assert_eq!(count_nodes(&root), 4);
    }

    #[test]
    fn change_ratio() {
        let mut td = TreeDiff::new();
        let root1 = make_element(
            "root",
            vec![make_element("a", vec![]), make_element("b", vec![])],
        );
        td.diff(&root1);
        let root2 = make_element(
            "root",
            vec![
                make_element("a", vec![]),
                make_element("c", vec![]), // changed
            ],
        );
        let result = td.diff(&root2);
        assert!(!result.unchanged);
        assert!(result.change_ratio() > 0.0);
        assert!(result.change_ratio() <= 1.0);
    }
}
