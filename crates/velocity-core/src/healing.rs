use std::collections::HashMap;
use std::sync::Mutex;

use tracing::{info, warn};
use velocity_common::{Element, PlatformDriver, Rect, Result, Selector, VelocityError};

/// Configuration for self-healing selectors.
#[derive(Debug, Clone)]
pub struct HealingConfig {
    /// Whether self-healing is enabled.
    pub enabled: bool,
    /// Minimum confidence (0.0–1.0) required to accept a healed match.
    pub confidence_threshold: f64,
    /// Whether to persist healed selector mappings.
    pub persist_healed: bool,
    /// Path to write healed mappings (if persist_healed is true).
    pub persist_path: Option<String>,
}

impl Default for HealingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            confidence_threshold: 0.8,
            persist_healed: true,
            persist_path: None,
        }
    }
}

/// A record of a healed selector match.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HealedMapping {
    /// The original selector that failed.
    pub original_selector: String,
    /// The healed selector that was found.
    pub healed_selector: String,
    /// Confidence score for the match.
    pub confidence: f64,
    /// Attributes of the matched element.
    pub matched_attributes: MatchedAttributes,
    /// Timestamp of when the healing occurred.
    pub healed_at: String,
}

/// Attributes of an element that was matched during healing.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MatchedAttributes {
    pub platform_id: String,
    pub label: Option<String>,
    pub text: Option<String>,
    pub element_type: String,
    pub bounds: [i32; 4],
}

impl From<&Element> for MatchedAttributes {
    fn from(el: &Element) -> Self {
        Self {
            platform_id: el.platform_id.clone(),
            label: el.label.clone(),
            text: el.text.clone(),
            element_type: el.element_type.clone(),
            bounds: [el.bounds.x, el.bounds.y, el.bounds.width, el.bounds.height],
        }
    }
}

/// A scored candidate element during healing.
#[derive(Debug)]
struct ScoredCandidate {
    element: Element,
    score: f64,
}

/// Self-healing selector engine that attempts to find elements
/// when the original selector fails by computing similarity scores
/// against all visible elements in the hierarchy.
pub struct SelectorHealer {
    config: HealingConfig,
    /// Previous element snapshots keyed by selector string.
    known_elements: Mutex<HashMap<String, ElementSnapshot>>,
    /// Accumulated healed mappings for persistence.
    healed_mappings: Mutex<Vec<HealedMapping>>,
}

/// Snapshot of an element's attributes plus surrounding context for fingerprinting.
#[derive(Debug, Clone)]
struct ElementSnapshot {
    #[allow(dead_code)]
    platform_id: String,
    label: Option<String>,
    text: Option<String>,
    element_type: String,
    bounds: Rect,
    /// Contextual fingerprint: parent type and sibling labels for structural matching.
    fingerprint: ElementFingerprint,
}

/// Captures structural context around an element for resilient matching.
/// Even if text/ID changes, position within the tree often stays stable.
#[derive(Debug, Clone, Default)]
struct ElementFingerprint {
    /// Parent element type (e.g., "Cell", "ScrollView").
    parent_type: Option<String>,
    /// Parent accessibility label.
    parent_label: Option<String>,
    /// Sibling element types (sorted for stable comparison).
    sibling_types: Vec<String>,
    /// Sibling labels/text (sorted, up to 5 nearest).
    #[allow(dead_code)]
    sibling_labels: Vec<String>,
    /// Index among siblings of the same type.
    type_index: Option<usize>,
}

impl From<&Element> for ElementSnapshot {
    fn from(el: &Element) -> Self {
        Self {
            platform_id: el.platform_id.clone(),
            label: el.label.clone(),
            text: el.text.clone(),
            element_type: el.element_type.clone(),
            bounds: el.bounds,
            fingerprint: ElementFingerprint::default(),
        }
    }
}

impl ElementSnapshot {
    /// Create a snapshot with structural context extracted from the hierarchy.
    fn with_context(element: &Element, hierarchy: &Element) -> Self {
        let fingerprint = extract_fingerprint(element, hierarchy);
        Self {
            platform_id: element.platform_id.clone(),
            label: element.label.clone(),
            text: element.text.clone(),
            element_type: element.element_type.clone(),
            bounds: element.bounds,
            fingerprint,
        }
    }
}

/// Extract fingerprint by finding the element's parent and siblings in the hierarchy.
fn extract_fingerprint(target: &Element, root: &Element) -> ElementFingerprint {
    if let Some((parent, siblings)) = find_parent_and_siblings(target, root) {
        let mut sibling_types: Vec<String> = siblings
            .iter()
            .map(|s| s.element_type.clone())
            .collect();
        sibling_types.sort();

        let mut sibling_labels: Vec<String> = siblings
            .iter()
            .filter_map(|s| s.label.clone().or_else(|| s.text.clone()))
            .take(5)
            .collect();
        sibling_labels.sort();

        let type_index = siblings
            .iter()
            .filter(|s| s.element_type == target.element_type)
            .position(|s| s.platform_id == target.platform_id || s.bounds == target.bounds);

        ElementFingerprint {
            parent_type: Some(parent.element_type.clone()),
            parent_label: parent.label.clone(),
            sibling_types,
            sibling_labels,
            type_index,
        }
    } else {
        ElementFingerprint::default()
    }
}

/// Walk the tree to find the parent of a target element and its siblings.
fn find_parent_and_siblings<'a>(
    target: &Element,
    current: &'a Element,
) -> Option<(&'a Element, Vec<&'a Element>)> {
    for child in &current.children {
        // Check if this child matches the target
        if child.platform_id == target.platform_id && child.bounds == target.bounds {
            let siblings: Vec<&Element> = current
                .children
                .iter()
                .filter(|c| !(c.platform_id == target.platform_id && c.bounds == target.bounds))
                .collect();
            return Some((current, siblings));
        }
        // Recurse
        if let Some(found) = find_parent_and_siblings(target, child) {
            return Some(found);
        }
    }
    None
}

impl SelectorHealer {
    pub fn new(config: HealingConfig) -> Self {
        Self {
            config,
            known_elements: Mutex::new(HashMap::new()),
            healed_mappings: Mutex::new(Vec::new()),
        }
    }

    /// Record a successful element find so we have attributes for future healing.
    pub fn record_success(&self, selector: &Selector, element: &Element) {
        if !self.config.enabled {
            return;
        }
        let key = format!("{selector}");
        if let Ok(mut known) = self.known_elements.lock() {
            known.insert(key, ElementSnapshot::from(element));
        }
    }

    /// Record a successful element find with full hierarchy context for fingerprinting.
    pub fn record_success_with_context(
        &self,
        selector: &Selector,
        element: &Element,
        hierarchy: &Element,
    ) {
        if !self.config.enabled {
            return;
        }
        let key = format!("{selector}");
        if let Ok(mut known) = self.known_elements.lock() {
            known.insert(key, ElementSnapshot::with_context(element, hierarchy));
        }
    }

    /// Attempt to heal a failed selector by searching the hierarchy.
    ///
    /// Returns the healed element and its confidence score, or None if
    /// no match above the confidence threshold was found.
    pub async fn try_heal(
        &self,
        driver: &dyn PlatformDriver,
        device_id: &str,
        selector: &Selector,
    ) -> Option<(Element, f64)> {
        if !self.config.enabled {
            return None;
        }

        let selector_str = format!("{selector}");

        // Get the hierarchy
        let hierarchy = match driver.get_hierarchy(device_id).await {
            Ok(h) => h,
            Err(_) => return None,
        };

        // Flatten the hierarchy into visible leaf/actionable elements
        let mut candidates = Vec::new();
        flatten_elements(&hierarchy, &mut candidates);

        if candidates.is_empty() {
            return None;
        }

        // Score each candidate
        let known = self.known_elements.lock().ok()?;
        let snapshot = known.get(&selector_str);

        let mut scored: Vec<ScoredCandidate> = candidates
            .into_iter()
            .map(|el| {
                let score = compute_similarity(selector, snapshot, &el, Some(&hierarchy));
                ScoredCandidate { element: el, score }
            })
            .filter(|sc| sc.score > 0.0)
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let best = scored.into_iter().next()?;

        if best.score < self.config.confidence_threshold {
            warn!(
                selector = %selector_str,
                best_score = best.score,
                threshold = self.config.confidence_threshold,
                "self-healing: best match below confidence threshold"
            );
            return None;
        }

        let healed_selector = describe_element(&best.element);
        info!(
            original = %selector_str,
            healed = %healed_selector,
            confidence = best.score,
            "self-healing: selector healed successfully"
        );

        // Record the healing
        drop(known); // release lock before acquiring another
        self.record_healing(&selector_str, &healed_selector, best.score, &best.element);

        Some((best.element, best.score))
    }

    fn record_healing(&self, original: &str, healed: &str, confidence: f64, element: &Element) {
        let mapping = HealedMapping {
            original_selector: original.to_string(),
            healed_selector: healed.to_string(),
            confidence,
            matched_attributes: MatchedAttributes::from(element),
            healed_at: chrono::Utc::now().to_rfc3339(),
        };

        if let Ok(mut mappings) = self.healed_mappings.lock() {
            mappings.push(mapping);
        }
    }

    /// Get all accumulated healed mappings (for persistence/reporting).
    pub fn healed_mappings(&self) -> Vec<HealedMapping> {
        self.healed_mappings
            .lock()
            .map(|m| m.clone())
            .unwrap_or_default()
    }

    /// Persist healed mappings to disk as JSON.
    pub fn persist(&self) -> Result<()> {
        if !self.config.persist_healed {
            return Ok(());
        }

        let path = self
            .config
            .persist_path
            .as_deref()
            .unwrap_or("velocity-healed-selectors.json");

        let mappings = self.healed_mappings();
        if mappings.is_empty() {
            return Ok(());
        }

        let json = serde_json::to_string_pretty(&mappings).map_err(|e| {
            VelocityError::Config(format!("Failed to serialize healed mappings: {e}"))
        })?;

        std::fs::write(path, json).map_err(|e| {
            VelocityError::Config(format!("Failed to write healed mappings to {path}: {e}"))
        })?;

        info!(
            path,
            count = mappings.len(),
            "persisted healed selector mappings"
        );
        Ok(())
    }

    /// Whether healing is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

/// Flatten a hierarchy tree into a list of visible elements.
fn flatten_elements(element: &Element, out: &mut Vec<Element>) {
    if element.visible && !element.bounds.is_empty() {
        // Include elements that have some identifying information
        if element.text.is_some() || element.label.is_some() || !element.platform_id.is_empty() {
            out.push(element.clone());
        }
    }
    for child in &element.children {
        flatten_elements(child, out);
    }
}

/// Compute a similarity score (0.0–1.0) between a selector/snapshot and a candidate element.
/// The hierarchy is used for fingerprint context matching.
fn compute_similarity(
    selector: &Selector,
    snapshot: Option<&ElementSnapshot>,
    candidate: &Element,
    hierarchy: Option<&Element>,
) -> f64 {
    let mut score = 0.0;
    let mut max_score = 0.0;

    // Score based on the selector type (what we were looking for)
    match selector {
        Selector::Id(id) => {
            max_score += 1.0;
            score += string_similarity(id, &candidate.platform_id);
        }
        Selector::Text(text) => {
            max_score += 1.0;
            let candidate_text = candidate.text.as_deref().unwrap_or("");
            score += string_similarity(text, candidate_text);
        }
        Selector::TextContains(sub) => {
            max_score += 1.0;
            let candidate_text = candidate.text.as_deref().unwrap_or("");
            if candidate_text.to_lowercase().contains(&sub.to_lowercase()) {
                score += 1.0;
            } else {
                score += string_similarity(sub, candidate_text) * 0.7;
            }
        }
        Selector::AccessibilityId(aid) => {
            max_score += 1.0;
            let candidate_label = candidate.label.as_deref().unwrap_or("");
            score += string_similarity(aid, candidate_label);
        }
        Selector::ClassName(cls) => {
            max_score += 1.0;
            score += string_similarity(cls, &candidate.element_type);
        }
        Selector::Compound(selectors) => {
            for sub_sel in selectors {
                let sub_score = compute_similarity(sub_sel, None, candidate, hierarchy);
                max_score += 1.0;
                score += sub_score;
            }
        }
        Selector::Index {
            selector: inner, ..
        } => {
            return compute_similarity(inner, snapshot, candidate, hierarchy);
        }
    }

    // If we have a previous snapshot, also compare against those attributes
    if let Some(snap) = snapshot {
        // Element type match (weighted)
        max_score += 0.5;
        if snap.element_type == candidate.element_type {
            score += 0.5;
        }

        // Text similarity
        if let (Some(snap_text), Some(cand_text)) = (&snap.text, &candidate.text) {
            max_score += 0.5;
            score += string_similarity(snap_text, cand_text) * 0.5;
        }

        // Label similarity
        if let (Some(snap_label), Some(cand_label)) = (&snap.label, &candidate.label) {
            max_score += 0.5;
            score += string_similarity(snap_label, cand_label) * 0.5;
        }

        // Spatial proximity (closer bounds = higher score)
        max_score += 0.3;
        score += spatial_similarity(&snap.bounds, &candidate.bounds) * 0.3;

        // Fingerprint context: parent and sibling matching
        score += fingerprint_similarity(&snap.fingerprint, candidate, hierarchy) * 0.4;
        max_score += 0.4;
    }

    if max_score == 0.0 {
        return 0.0;
    }

    score / max_score
}

/// Compute string similarity using Levenshtein-like normalized distance.
fn string_similarity(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    let a_lower = a.to_lowercase();
    let b_lower = b.to_lowercase();

    if a_lower == b_lower {
        return 0.95; // case-insensitive exact match
    }

    // Check containment
    if b_lower.contains(&a_lower) || a_lower.contains(&b_lower) {
        let shorter = a_lower.len().min(b_lower.len()) as f64;
        let longer = a_lower.len().max(b_lower.len()) as f64;
        return 0.7 + 0.2 * (shorter / longer);
    }

    // Levenshtein distance
    let distance = levenshtein(&a_lower, &b_lower);
    let max_len = a_lower.len().max(b_lower.len()) as f64;
    let similarity = 1.0 - (distance as f64 / max_len);

    similarity.max(0.0)
}

/// Compute spatial similarity between two rects (0.0–1.0).
fn spatial_similarity(a: &Rect, b: &Rect) -> f64 {
    let (ax, ay) = a.center();
    let (bx, by) = b.center();

    let dx = (ax - bx).abs() as f64;
    let dy = (ay - by).abs() as f64;
    let distance = (dx * dx + dy * dy).sqrt();

    // Normalize against a reference distance (half screen diagonal ~1000px)
    let max_distance = 1000.0;
    (1.0 - distance / max_distance).max(0.0)
}

/// Compute fingerprint similarity: how well does the candidate's structural
/// context (parent, siblings) match the snapshot's recorded context.
fn fingerprint_similarity(
    fingerprint: &ElementFingerprint,
    candidate: &Element,
    hierarchy: Option<&Element>,
) -> f64 {
    let hierarchy = match hierarchy {
        Some(h) => h,
        None => return 0.0,
    };

    let candidate_fp = extract_fingerprint(candidate, hierarchy);
    let mut score = 0.0;
    let mut max_score = 0.0;

    // Parent type match
    if let (Some(ref snap_parent), Some(ref cand_parent)) =
        (&fingerprint.parent_type, &candidate_fp.parent_type)
    {
        max_score += 1.0;
        if snap_parent == cand_parent {
            score += 1.0;
        }
    }

    // Parent label match
    if let (Some(ref snap_label), Some(ref cand_label)) =
        (&fingerprint.parent_label, &candidate_fp.parent_label)
    {
        max_score += 0.5;
        score += string_similarity(snap_label, cand_label) * 0.5;
    }

    // Sibling types overlap (Jaccard similarity)
    if !fingerprint.sibling_types.is_empty() || !candidate_fp.sibling_types.is_empty() {
        max_score += 0.5;
        let intersection = fingerprint
            .sibling_types
            .iter()
            .filter(|t| candidate_fp.sibling_types.contains(t))
            .count();
        let union = {
            let mut combined = fingerprint.sibling_types.clone();
            combined.extend(candidate_fp.sibling_types.iter().cloned());
            combined.sort();
            combined.dedup();
            combined.len()
        };
        if union > 0 {
            score += (intersection as f64 / union as f64) * 0.5;
        }
    }

    // Type index match (same position among same-type siblings)
    if let (Some(snap_idx), Some(cand_idx)) = (fingerprint.type_index, candidate_fp.type_index) {
        max_score += 0.3;
        if snap_idx == cand_idx {
            score += 0.3;
        }
    }

    if max_score == 0.0 {
        return 0.0;
    }
    score / max_score
}

/// Simple Levenshtein distance.
fn levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    let mut dp = vec![vec![0usize; n + 1]; m + 1];

    for (i, row) in dp.iter_mut().enumerate().take(m + 1) {
        row[0] = i;
    }
    for (j, val) in dp[0].iter_mut().enumerate().take(n + 1) {
        *val = j;
    }

    for i in 1..=m {
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }

    dp[m][n]
}

/// Generate a human-readable selector description for a healed element.
fn describe_element(element: &Element) -> String {
    if let Some(ref label) = element.label {
        if !label.is_empty() {
            return format!("accessibilityId={label:?}");
        }
    }
    if let Some(ref text) = element.text {
        if !text.is_empty() {
            return format!("text={text:?}");
        }
    }
    if !element.platform_id.is_empty() {
        return format!("id={:?}", element.platform_id);
    }
    format!(
        "type={:?} at ({},{})",
        element.element_type, element.bounds.x, element.bounds.y
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("", "hello"), 5);
        assert_eq!(levenshtein("same", "same"), 0);
    }

    #[test]
    fn test_string_similarity_exact() {
        assert_eq!(string_similarity("hello", "hello"), 1.0);
    }

    #[test]
    fn test_string_similarity_case_insensitive() {
        assert!(string_similarity("Hello", "hello") > 0.9);
    }

    #[test]
    fn test_string_similarity_containment() {
        let score = string_similarity("login", "loginButton");
        assert!(score > 0.7, "containment score was {score}");
    }

    #[test]
    fn test_string_similarity_different() {
        let score = string_similarity("apple", "orange");
        assert!(score < 0.5, "different string score was {score}");
    }

    #[test]
    fn test_spatial_similarity_same_position() {
        let r = Rect {
            x: 100,
            y: 200,
            width: 50,
            height: 50,
        };
        assert_eq!(spatial_similarity(&r, &r), 1.0);
    }

    #[test]
    fn test_spatial_similarity_far_apart() {
        let a = Rect {
            x: 0,
            y: 0,
            width: 50,
            height: 50,
        };
        let b = Rect {
            x: 900,
            y: 900,
            width: 50,
            height: 50,
        };
        let score = spatial_similarity(&a, &b);
        assert!(score < 0.2, "far apart score was {score}");
    }

    #[test]
    fn test_compute_similarity_exact_text_match() {
        let selector = Selector::Text("Sign In".to_string());
        let element = Element {
            platform_id: "btn1".to_string(),
            label: None,
            text: Some("Sign In".to_string()),
            element_type: "Button".to_string(),
            bounds: Rect {
                x: 100,
                y: 500,
                width: 200,
                height: 50,
            },
            enabled: true,
            visible: true,
            children: vec![],
        };
        let score = compute_similarity(&selector, None, &element, None);
        assert_eq!(score, 1.0);
    }

    #[test]
    fn test_compute_similarity_partial_match() {
        let selector = Selector::Text("Sign In".to_string());
        let element = Element {
            platform_id: "btn1".to_string(),
            label: None,
            text: Some("Sign In Here".to_string()),
            element_type: "Button".to_string(),
            bounds: Rect {
                x: 100,
                y: 500,
                width: 200,
                height: 50,
            },
            enabled: true,
            visible: true,
            children: vec![],
        };
        let score = compute_similarity(&selector, None, &element, None);
        assert!(score > 0.7, "partial match score was {score}");
    }

    #[test]
    fn test_flatten_elements() {
        let root = Element {
            platform_id: "root".to_string(),
            label: None,
            text: None,
            element_type: "View".to_string(),
            bounds: Rect {
                x: 0,
                y: 0,
                width: 1080,
                height: 2400,
            },
            enabled: true,
            visible: true,
            children: vec![
                Element {
                    platform_id: "btn1".to_string(),
                    label: Some("Login".to_string()),
                    text: Some("Login".to_string()),
                    element_type: "Button".to_string(),
                    bounds: Rect {
                        x: 100,
                        y: 500,
                        width: 200,
                        height: 50,
                    },
                    enabled: true,
                    visible: true,
                    children: vec![],
                },
                Element {
                    platform_id: "".to_string(),
                    label: None,
                    text: None,
                    element_type: "View".to_string(),
                    bounds: Rect {
                        x: 0,
                        y: 0,
                        width: 100,
                        height: 100,
                    },
                    enabled: true,
                    visible: true,
                    children: vec![],
                },
            ],
        };

        let mut flat = Vec::new();
        flatten_elements(&root, &mut flat);
        // root has platform_id so it's included; btn1 has label/text; the empty View has nothing
        assert_eq!(flat.len(), 2);
        assert_eq!(flat[1].platform_id, "btn1");
    }

    #[test]
    fn test_describe_element_prefers_label() {
        let el = Element {
            platform_id: "id1".to_string(),
            label: Some("Login Button".to_string()),
            text: Some("Login".to_string()),
            element_type: "Button".to_string(),
            bounds: Rect {
                x: 0,
                y: 0,
                width: 100,
                height: 50,
            },
            enabled: true,
            visible: true,
            children: vec![],
        };
        let desc = describe_element(&el);
        assert!(desc.contains("Login Button"));
    }
}
