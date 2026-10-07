//! Setting a property where the node has it (ARCHITECTURE §3.4, D14):
//! in its `style` when that's where it is said (that one declaration is
//! rewritten and the rest are kept), else as the attribute of its name,
//! which is also where a property the node didn't have goes. One that a
//! `<style>` rule gives the node goes into its `style` all the same: a
//! rule outvotes an attribute, and only the node's own style outvotes
//! the rule.

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
            let ruled = node.ruled(name, false).is_some() || node.ruled(name, true).is_some();
            let Some(value) = value.filter(|_| ruled) else { return self.set_attr(id, name, value) };
            // Said after whatever its style says already.
            let style = match node.attr("style").map(str::trim_end).filter(|s| !s.is_empty()) {
                Some(said) => format!("{said}{} {name}: {value}", if said.ends_with(';') { "" } else { ";" }),
                None => format!("{name}: {value}"),
            };
            return self.set_attr(id, "style", Some(&style));
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
                    // The first of the style takes the space after it
                    // along: what follows is the first now.
                    if rest.trim().is_empty() {
                        at += style[at..].len() - style[at..].trim_start().len();
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
    fn what_a_rule_gives_is_outvoted_only_in_the_nodes_own_style() {
        let ruled = |attrs: &str| {
            let attrs = if attrs.is_empty() { String::new() } else { format!(" {attrs}") };
            Document::parse(DocId(1), &format!("<svg><style>.a {{ fill: red; stroke-width: 2 }}</style><path class=\"a\"{attrs}/></svg>")).unwrap()
        };
        let after = |attrs: &str, name: &str, value: &str| {
            let mut d = ruled(attrs);
            assert!(d.set_prop(NodeId(3), name, Some(value)).unwrap());
            d.restyle();
            assert_eq!(prop(d.node(NodeId(3)).unwrap(), name), Some(value), "it now says so, whatever the rule says");
            d.markup(NodeId(3)).unwrap()
        };
        assert_eq!(after("", "fill", "blue"), r#"<path class="a" style="fill: blue"/>"#);
        assert_eq!(after(r#"fill="green" style="opacity: 0.5;""#, "stroke-width", "4"), r#"<path class="a" fill="green" style="opacity: 0.5; stroke-width: 4"/>"#);
        // What no rule gives it is an attribute as ever; taken off, the
        // rule is all that's left to say it.
        assert_eq!(after("", "opacity", "0.5"), r#"<path class="a" opacity="0.5"/>"#);
        let mut d = ruled(r#"style="fill: blue""#);
        assert!(d.set_prop(NodeId(3), "fill", None).unwrap());
        assert_eq!((d.markup(NodeId(3)).unwrap().as_str(), prop(d.node(NodeId(3)).unwrap(), "fill")), (r#"<path class="a"/>"#, Some("red")));
    }

    #[test]
    fn a_property_taken_off_goes_from_everywhere_it_was_said() {
        assert_eq!(after(r#"stroke-width="2""#, "stroke-width", None), "<path/>");
        assert_eq!(after(r#"style="fill:red; stroke: blue;""#, "fill", None), r#"<path style="stroke: blue;"/>"#, "the first goes with the space after it");
        assert_eq!(after(r#"style="fill:red; stroke: blue""#, "stroke", None), r#"<path style="fill:red"/>"#);
        assert_eq!(after(r#"style="fill:red;stroke:blue;fill:green" fill="black""#, "fill", None), r#"<path style="stroke:blue"/>"#);
        assert_eq!(after(r#"style=" fill : red ; ""#, "fill", None), "<path/>", "a style with nothing left in it goes too");
        assert_eq!(doc(r#"style="fill:red""#).set_prop(N, "stroke", None), Ok(false));
        assert!(doc("").set_prop(NodeId(9), "fill", None).is_err());
    }
}
