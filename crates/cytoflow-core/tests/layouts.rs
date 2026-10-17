//! Layouts and UI state are stored in the workspace file and survive a
//! save / load round trip; files without them load with empty defaults.

use cytoflow_core::workspace::Layout;
use cytoflow_core::Workspace;
use serde_json::{json, Value};

fn sample_layouts() -> Vec<Layout> {
    vec![
        Layout { id: "L1".into(), name: "Layout 1".into(), data: json!({ "page": "a4", "items": [{ "id": "i1", "kind": "plot", "x": 10, "y": 20 }] }) },
        Layout { id: "L2".into(), name: "Figure 2".into(), data: json!({ "page": "letter", "items": [] }) },
    ]
}

#[test]
fn layouts_and_ui_state_round_trip_through_json() {
    let mut ws = Workspace::new("rt");
    assert!(ws.layouts().is_empty());
    assert_eq!(ws.ui_state(), &Value::Null);
    ws.set_layouts(sample_layouts());
    let ui = json!({ "axes": { "s1": { "x": "FSC-A", "y": "SSC-A", "gate": "g3" } }, "overlay": ["s1", "s2"] });
    ws.set_ui_state(ui.clone());

    let text = ws.to_json().unwrap();
    let back = Workspace::from_json(&text).unwrap();
    assert_eq!(back.version, 2);
    assert_eq!(back.layouts(), sample_layouts().as_slice());
    assert_eq!(back.ui_state(), &ui);
}

#[test]
fn layouts_and_ui_state_round_trip_through_a_file() {
    let dir = std::env::temp_dir().join(format!("cytoflow-layouts-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("ws.cfw.json");
    let mut ws = Workspace::new("file");
    ws.set_layouts(sample_layouts());
    ws.set_ui_state(json!({ "k": 1 }));
    ws.save(&path).unwrap();
    let (back, errors) = Workspace::load(&path).unwrap();
    assert!(errors.is_empty());
    assert_eq!(back.layouts().len(), 2);
    assert_eq!(back.layouts()[1].name, "Figure 2");
    assert_eq!(back.ui_state(), &json!({ "k": 1 }));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn older_files_without_layouts_load_with_defaults() {
    let v2 = r#"{ "version": 2, "name": "old", "samples": [], "channel_settings": {}, "compensation": { "source": "none" } }"#;
    let ws = Workspace::from_json(v2).unwrap();
    assert!(ws.layouts().is_empty());
    assert_eq!(ws.ui_state(), &Value::Null);
    let v1 = r#"{ "version": 1, "name": "v1", "samples": [], "channel_settings": {}, "compensation": { "source": "none" }, "gates": [] }"#;
    let ws = Workspace::from_json(v1).unwrap();
    assert!(ws.layouts().is_empty());
}

#[test]
fn layout_data_defaults_to_null() {
    let l: Layout = serde_json::from_str(r#"{ "id": "a", "name": "A" }"#).unwrap();
    assert_eq!(l.data, Value::Null);
}
