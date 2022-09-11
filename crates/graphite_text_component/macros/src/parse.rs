

use super::*;
use crate::tree::RawNode;

#[derive(Default, Debug)]
pub struct RawComponent {
    // Values
    pub value: RawComponentValue,
    pub children: Vec<RawComponent>,

    // Formatting
    pub color: String,
    pub font: String,
    pub bold: bool,
    pub italic: bool,
    pub underlined: bool,
    pub strikethrough: bool,
    pub obfuscated: bool,
}

#[derive(Default, Debug)]
pub enum RawComponentValue {
    #[default]
    None,
    Text(String),
}

pub(crate) fn parse(nodes: Vec<RawNode>) -> Result<RawComponent, MacroError> {
    let mut root = RawComponent::default();
    parse_children(&mut root, nodes)?;
    Ok(root)
}

fn parse_children(component: &mut RawComponent, nodes: Vec<RawNode>) -> Result<(), MacroError> {
    for node in nodes {
        let mut child = RawComponent::default();
        parse_one(&mut child, node)?;
        component.children.push(child);
    }
    Ok(())
}

fn parse_one(component: &mut RawComponent, node: RawNode) -> Result<(), MacroError> {
    match node {
        RawNode::Macro {
            name,
            span,
            arguments,
            children,
        } => {
            parse_macro(component, name, span, arguments.into_iter().collect())?;
            parse_children(component, children)?;
        }
        RawNode::Variable { name: _, span: _ } => (),
        RawNode::Text { value: _ } => (),
    }
    Ok(())
}

fn parse_macro(
    component: &mut RawComponent,
    name: String,
    span: Span,
    arguments: Vec<TokenTree>,
) -> Result<(), MacroError> {
    match name.as_str() {
        "red" => component.color = "red".into(),
        "bold" => component.bold = true,
        "link" => {
            if arguments.len() != 1 {
                return Err(MacroError(
                    span,
                    format!("expected 1 argument, found {}", arguments.len()),
                ));
            }
        }
        _ => return Err(MacroError(span, format!("unknown function `{}`", name))),
    }
    Ok(())
}
