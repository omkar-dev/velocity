use quick_xml::events::Event;
use quick_xml::Reader;
use regex::Regex;
use std::collections::HashMap;

use super::xib::{XibDocument, XibParser};

/// Parses iOS Storyboard files to extract view hierarchies.
///
/// A storyboard contains multiple scenes, each with a view controller.
/// We find the initial view controller and parse its view hierarchy.
pub struct StoryboardParser;

impl StoryboardParser {
    /// Parse a storyboard XML and return the initial scene's view hierarchy.
    pub fn parse(xml: &str) -> Result<XibDocument, StoryboardError> {
        // Find the initial view controller ID
        let initial_vc_id = Self::find_initial_view_controller(xml)?;

        // Find the scene containing that view controller
        let scene_xml = Self::extract_scene(xml, &initial_vc_id)?;

        // Parse the scene as a XIB document
        XibParser::parse(&scene_xml).map_err(|e| StoryboardError::ParseError(e.to_string()))
    }

    /// Parse all scenes from the storyboard.
    pub fn parse_all_scenes(xml: &str) -> Result<Vec<(String, XibDocument)>, StoryboardError> {
        let scenes = Self::extract_all_scenes(xml)?;
        let mut results = Vec::new();

        for (id, scene_xml) in scenes {
            match XibParser::parse(&scene_xml) {
                Ok(doc) => results.push((id, doc)),
                Err(e) => {
                    tracing::warn!("Failed to parse scene {}: {}", id, e);
                }
            }
        }

        Ok(results)
    }

    fn find_initial_view_controller(xml: &str) -> Result<String, StoryboardError> {
        let mut reader = Reader::from_str(xml);

        loop {
            match reader.read_event() {
                Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                    let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    if tag == "document" {
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            if key == "initialViewController" {
                                return Ok(
                                    String::from_utf8_lossy(&attr.value).to_string()
                                );
                            }
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(StoryboardError::ParseError(e.to_string())),
                _ => {}
            }
        }

        Err(StoryboardError::NoInitialViewController)
    }

    fn extract_scene(xml: &str, vc_id: &str) -> Result<String, StoryboardError> {
        let scenes = Self::extract_all_scenes(xml)?;
        let id_pattern = Regex::new(&format!(r#"id\s*=\s*\"{}\""#, regex::escape(vc_id)))
            .map_err(|e| StoryboardError::ParseError(e.to_string()))?;
        for (id, scene_xml) in scenes {
            if id == vc_id || id_pattern.is_match(&scene_xml) {
                return Ok(scene_xml);
            }
        }
        Err(StoryboardError::SceneNotFound(vc_id.to_string()))
    }

    fn extract_all_scenes(xml: &str) -> Result<Vec<(String, String)>, StoryboardError> {
        let mut reader = Reader::from_str(xml);
        let mut scenes = Vec::new();
        let mut in_scene = false;
        let mut scene_depth = 0u32;
        let mut scene_id = String::new();
        let mut scene_content = String::new();

        loop {
            match reader.read_event() {
                Ok(Event::Start(e)) => {
                    let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    if tag == "scene" {
                        in_scene = true;
                        scene_depth = 1;
                        scene_content.clear();
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            if key == "sceneID" {
                                scene_id =
                                    String::from_utf8_lossy(&attr.value).to_string();
                            }
                        }
                    } else if in_scene {
                        scene_depth += 1;
                        // Reconstruct XML for this element
                        scene_content.push_str(&format!("<{}", tag));
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.as_ref());
                            let val = String::from_utf8_lossy(&attr.value);
                            scene_content.push_str(&format!(
                                " {}=\"{}\"",
                                key,
                                escape_xml_attr(&val)
                            ));
                        }
                        scene_content.push('>');
                    }
                }
                Ok(Event::Empty(e)) if in_scene => {
                    let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    scene_content.push_str(&format!("<{}", tag));
                    for attr in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(attr.key.as_ref());
                        let val = String::from_utf8_lossy(&attr.value);
                        scene_content.push_str(&format!(
                            " {}=\"{}\"",
                            key,
                            escape_xml_attr(&val)
                        ));
                    }
                    scene_content.push_str("/>");
                }
                Ok(Event::End(e)) => {
                    let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    if in_scene {
                        scene_depth -= 1;
                        if tag == "scene" && scene_depth == 0 {
                            in_scene = false;
                            scenes.push((scene_id.clone(), scene_content.clone()));
                        } else {
                            scene_content.push_str(&format!("</{}>", tag));
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(StoryboardError::ParseError(e.to_string())),
                _ => {}
            }
        }

        Ok(scenes)
    }
}

fn escape_xml_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[derive(Debug, thiserror::Error)]
pub enum StoryboardError {
    #[error("Storyboard parse error: {0}")]
    ParseError(String),
    #[error("No initial view controller found in storyboard")]
    NoInitialViewController,
    #[error("Scene not found for view controller: {0}")]
    SceneNotFound(String),
}
