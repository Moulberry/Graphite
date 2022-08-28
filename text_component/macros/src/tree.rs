use std::iter::Peekable;

use super::*;
use proc_macro2::{Delimiter, Span, TokenStream, TokenTree};

#[derive(Debug)]
pub enum RawNode {
    Macro {
        name: String,
        span: Span,
        arguments: TokenStream,
        children: Vec<RawNode>,
    },
    Variable {
        name: String,
        span: Span,
    },
    Text {
        value: String,
    },
}

fn parse_arguments_if_parenthesis(
    tokens: &mut Peekable<impl Iterator<Item = TokenTree>>,
) -> TokenStream {
    let token = tokens.peek();
    if let Some(token) = token {
        match token {
            TokenTree::Group(group) => {
                if group.delimiter() == Delimiter::Parenthesis {
                    let stream = group.stream();
                    tokens.next();
                    return stream;
                }
            }
            _ => (),
        }
    }
    Default::default()
}

fn parse_one(
    tokens: &mut Peekable<impl Iterator<Item = TokenTree>>,
) -> Result<Vec<RawNode>, MacroError> {
    let mut nodes = Vec::new();

    let token = tokens.next();
    if let Some(token) = token {
        match token {
            TokenTree::Group(group) => match group.delimiter() {
                Delimiter::Brace => {
                    let inner_tokens = group.stream().into_iter().peekable();
                    return parse_all(inner_tokens);
                }
                Delimiter::Parenthesis => {
                    return Err(MacroError(group.span(), "unexpected parenthesis".into()));
                }
                Delimiter::Bracket => {
                    return Err(MacroError(group.span(), "unexpected brackets".into()));
                }
                Delimiter::None => panic!("found implicit delimiter where there shouldn't be any"),
            },
            TokenTree::Ident(ident) => {
                // Check if the following token is a bang (!)
                // If so, push a macro node
                match tokens.peek() {
                    Some(next_token) => match next_token {
                        TokenTree::Punct(punct) => {
                            if punct.as_char() == '!' {
                                tokens.next();
                                nodes.push(RawNode::Macro {
                                    name: ident.to_string(),
                                    span: ident.span(),
                                    arguments: parse_arguments_if_parenthesis(tokens),
                                    children: parse_one(tokens)?,
                                });
                                return Ok(nodes);
                            }
                        }
                        _ => (),
                    },
                    None => (),
                }
                // Otherwise, push a variable node
                nodes.push(RawNode::Variable {
                    name: ident.to_string(),
                    span: ident.span(),
                });
            }
            TokenTree::Punct(punct) => {
                return Err(MacroError(punct.span(), "unexpected punctuation".into()))
            } // error
            TokenTree::Literal(literal) => {
                nodes.push(RawNode::Text {
                    value: literal.to_string(),
                });
                return Ok(nodes);
            }
        }
    }
    Ok(nodes)
}

pub(crate) fn parse_all(
    mut tokens: Peekable<impl Iterator<Item = TokenTree>>,
) -> Result<Vec<RawNode>, MacroError> {
    let mut nodes = Vec::new();

    loop {
        if tokens.peek().is_none() {
            return Ok(nodes);
        }

        let new_nodes = parse_one(&mut tokens)?;
        nodes.extend(new_nodes);
    }
}
