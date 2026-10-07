//! Setting a property where the node has it (ARCHITECTURE §3.4, D14):
//! in its `style` when that's where it is said (that one declaration is
//! rewritten and the rest are kept), else as the attribute of its name,
//! which is also where a property the node didn't have goes.

use crate::document::Document;
use crate::error::DocError;
use crate::id::NodeId;
use crate::style::declarations;

impl Document {
    /// Set the property `name` of `id` to `value`, or take it off
    /// (`None`): from its `style` and its attribute both, since either
    /// would still say it. Whether that changed anything.
    pub(crate) fn set_prop(&mut self, id: NodeId, name: &str, value: Option<&str>) -> Result<bool, DocError> {
        let node = self.node(id)?;
        let Some(style) = node.attr("style").filter(|style| declarations(style).any(|d| d.name == name)) else {
            return self.set_attr(id, name, value);
        };
        match value {
            Some(value) => {
                // The last one of that name is the one that counts.
                let last = declarations(style).filter(|d| d.name == name).last().expect("it has one");
                let rewritten = format!("{}{value}{}", &style[..last.value_at.start], &style[last.value_at.end..]);
                self.set_attr(id, "style", Some(&rewritten))
            }
            None => {
                let gone: Vec<std::ops::Range<usize>> = declarations(style).filter(|d| d.name == name).map(|d| d.at).collect();
                // Each goes with the `;` after it (or, for the last of
                // the style, the one before).
                let mut rest = String::with_capacity(style.len());
                let mut at = 0;
                for range in gone {
                    rest.push_str(&style[at..range.start]);
                    at = (range.end + 1).min(style.len());
                    if range.end >= style.len() && rest.trim_end().ends_with(';') {
                        rest.truncate(rest.trim_end().len() - 1);
                    }
                }
                rest.push_str(&style[at..]);
                let style_changed = self.set_attr(id, "style", if rest.trim().is_empty() { None } else { Some(&rest) })?;
                Ok(self.set_attr(id, name, None)? || style_changed)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::DocId;
    use crate::style::prop;

    const N: NodeId = NodeId(2);

    fn doc(attrs: &str) -> Document {
        Document::parse(DocId(1), &format!("<svg><path {attrs}/></svg>")).unwrap()
    }

    fn after(attrs: &str, name: &str, value: Option<&str>) -> String {
        let mut d = doc(attrs);
        assert!(d.set_prop(N, name, value).unwrap(), "{attrs}: {name} changed");
        assert_eq!(prop(d.node(N).unwrap(), name), value, "{attrs}: it now says so");
        d.markup(N).unwrap()
    }

    #[test]
    fn a_property_is_set_where_the_node_has_it() {
        assert_eq!(after(r#"stroke-width="2""#, "stroke-width", Some("4")), r#"<path stroke-width="4"/>"#);
        assert_eq!(after(r#"style="fill:red;  stroke-width : 2px ; stroke: blue""#, "stroke-width", Some("4")), r#"<path style="fill:red;  stroke-width : 4 ; stroke: blue"/>"#);
        // In its style, though an attribute says it too: the style wins.
        assert_eq!(after(r#"stroke-width="1" style="stroke-width:2!important""#, "stroke-width", Some("4")), r#"<path stroke-width="1" style="stroke-width:4!important"/>"#);
        // Said twice in a style, the later one counts.
        assert_eq!(after(r#"style="fill:red;fill:green""#, "fill", Some("blue")), r#"<path style="fill:red;fill:blue"/>"#);
        // One it didn't have goes in as an attribute.
        assert_eq!(after(r#"style="fill:red""#, "stroke-width", Some("4")), r#"<path style="fill:red" stroke-width="4"/>"#);
        assert_eq!(doc(r#"style="fill:red""#).set_prop(N, "fill", Some("red")), Ok(false), "the same again changes nothing");
    }

    #[test]
    fn a_property_taken_off_goes_from_everywhere_it_was_said() {
        assert_eq!(after(r#"stroke-width="2""#, "stroke-width", None), "<path/>");
        assert_eq!(after(r#"style="fill:red; stroke: blue;""#, "fill", None), r#"<path style=" stroke: blue;"/>"#);
        assert_eq!(after(r#"style="fill:red; stroke: blue""#, "stroke", None), r#"<path style="fill:red"/>"#);
        assert_eq!(after(r#"style="fill:red;stroke:blue;fill:green" fill="black""#, "fill", None), r#"<path style="stroke:blue"/>"#);
        assert_eq!(after(r#"style=" fill : red ; ""#, "fill", None), "<path/>", "a style with nothing left in it goes too");
        assert_eq!(doc(r#"style="fill:red""#).set_prop(N, "stroke", None), Ok(false));
        assert!(doc("").set_prop(NodeId(9), "fill", None).is_err());
    }
}
