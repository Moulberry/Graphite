use std::collections::BTreeMap;
use std::fmt::Debug;
use std::{collections::HashMap, result};

use thiserror::Error;

use crate::types::DispatchFunction;

use super::parsers::MinecraftParser;
use super::parsers::NumericParser;
use super::parsers::StringParser;

// Merge error enum

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MergeError {
    #[error("DispatchNode already had an executor, merge attempted to override")]
    DuplicateExecutor,
    #[error("New parser conflicts with existing parser")]
    AmbiguousNewParser,
    #[error("New parser contained an alias with a duplicate key")]
    DuplicateAlias,
}

#[derive(Debug)]
pub struct MinecraftRootDispatchNode {
    pub literals: HashMap<&'static str, MinecraftDispatchNode>,
    pub aliases: HashMap<&'static str, &'static str>,
}

impl MinecraftRootDispatchNode {
    #[cfg(test)]
    pub(crate) fn merge_named(
        &mut self,
        dispatch: MinecraftDispatchNode,
        name: &'static str,
        aliases: Vec<&'static str>
    ) -> result::Result<(), MergeError> {
        // Create alias map
        let mut aliases_map = HashMap::new();
        for alias in aliases {
            aliases_map.insert(alias, name);
        }

        // Create root dispatch node
        let other = MinecraftRootDispatchNode {
            literals: maplit::hashmap! {
                name => dispatch
            },
            aliases: aliases_map,
        };

        // Do the merge
        self.merge(other)
    }

    pub fn merge(
        &mut self,
        other: MinecraftRootDispatchNode,
    ) -> result::Result<(), MergeError> {
        // Try to merge literals
        if self.literals.is_empty() {
            self.literals = other.literals;
        } else if !other.literals.is_empty() {
            for new_literal in other.literals {
                if let Some(existing_literal) = self.literals.get_mut(new_literal.0) {
                    // Merge with existing literal
                    existing_literal.merge(new_literal.1)?;
                } else {
                    // No conflict with existing, insert into `literals`
                    self.literals.insert(new_literal.0, new_literal.1);
                }
            }
        }

        // Try to merge aliases
        if self.aliases.is_empty() {
            self.aliases = other.aliases;
        } else if other.aliases.is_empty() {
            for new_alias in other.aliases {
                if let Some(existing_alias) = self.aliases.get(new_alias.0) {
                    // Check if value is the same
                    if new_alias.1 != *existing_alias {
                        return Err(MergeError::DuplicateAlias);
                    }
                } else {
                    // Insert the new alias
                    self.aliases.insert(new_alias.0, new_alias.1);
                }
            }
        }

        Ok(())
    }
}

#[derive(Clone)]
pub struct MinecraftDispatchNode {
    pub literals: BTreeMap<&'static str, MinecraftDispatchNode>,
    pub aliases: BTreeMap<&'static str, &'static str>,
    pub numeric_parser: Option<MinecraftArgumentNode<NumericParser>>,
    pub string_parser: Option<MinecraftArgumentNode<StringParser>>,
    pub executor: Option<DispatchFunction>,
}

impl Debug for MinecraftDispatchNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MinecraftDispatchNode")
            .field("literals", &self.literals)
            .field("aliases", &self.aliases)
            .field("numeric_parser", &self.numeric_parser)
            .field("string_parser", &self.string_parser)
            .field("has_executor", &self.executor.is_some())
            .finish()
    }
}

impl MinecraftDispatchNode {
    pub(crate) fn merge(&mut self, node: MinecraftDispatchNode) -> result::Result<(), MergeError> {
        // Try to merge executor
        if self.executor.is_none() {
            self.executor = node.executor;
        } else if node.executor.is_some() {
            // Both self.executor and node.executor exist
            return Err(MergeError::DuplicateExecutor);
        }

        // Merge the numeric parser
        if let Some(new_numeric_parser) = node.numeric_parser {
            if let Some(numeric_parser) = self.numeric_parser.as_mut() {
                if numeric_parser.parse.is_equal(new_numeric_parser.parse) {
                    numeric_parser
                        .dispatch_node
                        .merge(*new_numeric_parser.dispatch_node)?;
                }
            } else {
                self.numeric_parser = Some(new_numeric_parser);
            }
        }

        // Merge the string parser
        if let Some(new_string_parser) = node.string_parser {
            if let Some(string_parser) = self.string_parser.as_mut() {
                if string_parser.parse.is_equal(new_string_parser.parse) {
                    string_parser
                        .dispatch_node
                        .merge(*new_string_parser.dispatch_node)?;
                }
            } else {
                self.string_parser = Some(new_string_parser);
            }
        }

        // Try to merge literals
        if self.literals.is_empty() {
            self.literals = node.literals;
        } else if !node.literals.is_empty() {
            for new_literal in node.literals {
                if let Some(existing_literal) = self.literals.get_mut(new_literal.0) {
                    // Merge with existing literal
                    existing_literal.merge(new_literal.1)?;
                } else {
                    // No conflict with existing, insert into `literals`
                    self.literals.insert(new_literal.0, new_literal.1);
                }
            }
        }

        // Try to merge aliases
        if self.aliases.is_empty() {
            self.aliases = node.aliases;
        } else if node.aliases.is_empty() {
            for new_alias in node.aliases {
                if let Some(existing_alias) = self.aliases.get(new_alias.0) {
                    // Check if value is the same
                    if new_alias.1 != *existing_alias {
                        return Err(MergeError::DuplicateAlias);
                    }
                } else {
                    // Insert the new alias
                    self.aliases.insert(new_alias.0, new_alias.1);
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct MinecraftArgumentNode<P: MinecraftParser> {
    pub name: &'static str,
    pub parse: P,
    pub dispatch_node: Box<MinecraftDispatchNode>,
}

// Tests

#[cfg(test)]
fn empty_root() -> MinecraftRootDispatchNode {
    MinecraftRootDispatchNode {
        literals: HashMap::new(),
        aliases: HashMap::new(),
    }
}

#[cfg(test)]
fn empty_dispatch_node() -> MinecraftDispatchNode {
    MinecraftDispatchNode {
        literals: BTreeMap::new(),
        aliases: BTreeMap::new(),
        numeric_parser: None,
        string_parser: None,
        executor: None,
    }
}

#[cfg(test)]
fn dispatch_node_with_numeric_parser<'a>(dispatch: MinecraftDispatchNode) -> MinecraftDispatchNode {
    let numeric_parser = MinecraftArgumentNode {
        name: "argument",
        parse: NumericParser::U8 { min: u8::MIN, max: u8::MAX },
        dispatch_node: Box::from(dispatch),
    };
    MinecraftDispatchNode {
        literals: BTreeMap::new(),
        aliases: BTreeMap::new(),
        numeric_parser: Some(numeric_parser),
        string_parser: None,
        executor: None,
    }
}

#[cfg(test)]
fn dispatch_node_with_string_parser(dispatch: MinecraftDispatchNode) -> MinecraftDispatchNode {
    let string_parser = MinecraftArgumentNode {
        name: "argument",
        parse: StringParser::Word,
        dispatch_node: Box::from(dispatch),
    };
    MinecraftDispatchNode {
        literals: BTreeMap::new(),
        aliases: BTreeMap::new(),
        numeric_parser: None,
        string_parser: Some(string_parser),
        executor: None,
    }
}

#[cfg(test)]
fn dispatch_node_with_executor() -> MinecraftDispatchNode {
    use crate::types::{Span, CommandDispatchResult};
    fn hello(_: &[u8], _: &[Span]) -> CommandDispatchResult { CommandDispatchResult::Success(Ok(())) }

    MinecraftDispatchNode {
        literals: BTreeMap::new(),
        aliases: BTreeMap::new(),
        numeric_parser: None,
        string_parser: None,
        executor: Some(hello),
    }
}

#[test]
fn simple_merge() {
    let mut root = empty_root();

    let dispatch = dispatch_node_with_executor();
    assert_eq!(root.merge_named(dispatch, "hello", vec![]), Ok(()));
    assert_eq!(root.literals.len(), 1);
}

#[test]
fn merge_alias() {
    let mut root = empty_root();

    let dispatch = dispatch_node_with_executor();
    assert_eq!(
        root.merge_named(dispatch, "hello", vec!["hello1", "hello2"]),
        Ok(())
    );

    assert_eq!(root.literals.len(), 1);
    assert_eq!(root.aliases.len(), 2);

    let dispatch = dispatch_node_with_executor();
    assert_eq!(root.merge_named(dispatch, "bye", vec!["bye1", "bye2"]), Ok(()));

    assert_eq!(root.literals.len(), 2);
    assert_eq!(root.aliases.len(), 4);
}

#[test]
fn merge_empty() {
    let mut root = empty_root();

    let dispatch = empty_dispatch_node();
    assert_eq!(
        root.merge_named(dispatch, "hello", vec!["hello1", "hello2"]),
        Ok(())
    );

    let dispatch = empty_dispatch_node();
    assert_eq!(
        root.merge_named(dispatch, "hello", vec!["hello1", "hello2"]),
        Ok(())
    );

    assert_eq!(root.literals.len(), 1);
    assert_eq!(root.aliases.len(), 2);
}

#[test]
fn merge_separate_parsers() {
    let mut root = empty_root();

    // Merge a numeric parser
    let dispatch = dispatch_node_with_numeric_parser(dispatch_node_with_executor());
    assert_eq!(root.merge_named(dispatch, "hello", vec![]), Ok(()));

    // Merge a string parser
    let dispatch = dispatch_node_with_string_parser(dispatch_node_with_executor());
    assert_eq!(root.merge_named(dispatch, "hello", vec![]), Ok(()));

    assert_eq!(root.literals.len(), 1);

    let literal = root.literals.iter().next().unwrap();
    assert_eq!(*literal.0, "hello");

    assert!(literal.1.numeric_parser.is_some());
    assert!(literal.1.string_parser.is_some());
}

#[test]
fn merge_incompatible_parsers() {
    let mut root = empty_root();

    // Merge a numeric parser
    let dispatch = dispatch_node_with_numeric_parser(dispatch_node_with_executor());
    assert_eq!(root.merge_named(dispatch.clone(), "hello", vec![]), Ok(()));
    assert_eq!(root.literals.len(), 1);

    assert_eq!(
        root.merge_named(dispatch, "hello", vec![]),
        Err(MergeError::DuplicateExecutor)
    );
}

#[test]
fn merge_compatible_parsers() {
    let mut root = empty_root();

    // Merge a numeric parser
    let dispatch = dispatch_node_with_numeric_parser(empty_dispatch_node());
    assert_eq!(root.merge_named(dispatch.clone(), "hello", vec![]), Ok(()));
    assert_eq!(root.literals.len(), 1);

    assert_eq!(root.merge_named(dispatch, "hello", vec![]), Ok(()));
}

#[test]
fn duplicate_executor_merge() {
    let mut root = empty_root();

    let dispatch = dispatch_node_with_executor();
    assert_eq!(root.merge_named(dispatch, "hello", vec![]), Ok(()));

    let dispatch = dispatch_node_with_executor();
    assert_eq!(
        root.merge_named(dispatch, "hello", vec![]),
        Err(MergeError::DuplicateExecutor)
    );
}

#[test]
fn duplicate_alias_merge() {
    let mut root = empty_root();

    let dispatch = dispatch_node_with_executor();
    assert_eq!(
        root.merge_named(dispatch, "hello", vec!["hello1", "special"]),
        Ok(())
    );

    let dispatch = dispatch_node_with_executor();
    assert_eq!(
        root.merge_named(dispatch, "world", vec!["world1", "special"]),
        Err(MergeError::DuplicateAlias)
    );
}
