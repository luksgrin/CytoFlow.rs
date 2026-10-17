//! Minimal XML DOM on top of quick-xml, shared by the Gating-ML and FlowJo
//! workspace readers.

use crate::error::{Error, Result};
use quick_xml::events::Event;
use quick_xml::Reader;

#[derive(Debug, Clone, Default)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Element>,
    pub text: String,
}

impl Element {
    /// Local name without namespace prefix.
    pub fn local(&self) -> &str {
        self.name.rsplit(':').next().unwrap_or(&self.name)
    }

    pub fn is(&self, local: &str) -> bool {
        self.local() == local
    }

    /// Attribute lookup by local name (ignores namespace prefix).
    pub fn attr(&self, local: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k.rsplit(':').next().unwrap_or(k) == local)
            .map(|(_, v)| v.as_str())
    }

    pub fn attr_f64(&self, local: &str) -> Option<f64> {
        self.attr(local).and_then(|v| v.trim().parse().ok())
    }

    pub fn child(&self, local: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.is(local))
    }

    pub fn children_named<'a>(&'a self, local: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |c| c.is(local))
    }

    /// Depth-first search for all descendants with the given local name.
    pub fn descendants<'a>(&'a self, local: &'a str) -> Vec<&'a Element> {
        let mut out = Vec::new();
        fn walk<'a>(e: &'a Element, local: &str, out: &mut Vec<&'a Element>) {
            for c in &e.children {
                if c.is(local) {
                    out.push(c);
                }
                walk(c, local, out);
            }
        }
        walk(self, local, &mut out);
        out
    }
}

pub fn parse(xml: &str) -> Result<Element> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut stack: Vec<Element> = vec![Element { name: "#document".into(), ..Default::default() }];
    loop {
        match reader.read_event()? {
            Event::Start(e) => {
                let el = make_element(&e)?;
                stack.push(el);
            }
            Event::Empty(e) => {
                let el = make_element(&e)?;
                stack.last_mut().unwrap().children.push(el);
            }
            Event::End(_) => {
                let el = stack.pop().ok_or_else(|| Error::Xml("unbalanced end tag".into()))?;
                stack.last_mut().ok_or_else(|| Error::Xml("unbalanced end tag".into()))?.children.push(el);
            }
            Event::Text(t) => {
                let txt = t.unescape().map_err(|e| Error::Xml(e.to_string()))?;
                stack.last_mut().unwrap().text.push_str(&txt);
            }
            Event::CData(t) => {
                stack.last_mut().unwrap().text.push_str(&String::from_utf8_lossy(&t));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let mut doc = stack.pop().ok_or_else(|| Error::Xml("empty document".into()))?;
    if !stack.is_empty() {
        return Err(Error::Xml("unterminated element".into()));
    }
    doc.children.pop().ok_or_else(|| Error::Xml("no root element".into()))
}

fn make_element(e: &quick_xml::events::BytesStart<'_>) -> Result<Element> {
    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
    let mut attrs = Vec::new();
    for a in e.attributes() {
        let a = a.map_err(|e| Error::Xml(e.to_string()))?;
        let k = String::from_utf8_lossy(a.key.as_ref()).to_string();
        let v = a.unescape_value().map_err(|e| Error::Xml(e.to_string()))?.to_string();
        attrs.push((k, v));
    }
    Ok(Element { name, attrs, children: Vec::new(), text: String::new() })
}
