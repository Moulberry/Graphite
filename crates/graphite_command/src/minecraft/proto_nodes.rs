use std::collections::BTreeMap;
use std::fmt::Debug;
use std::{collections::HashMap, result};

use graphite_mc_protocol::types::SuggestionType;
use thiserror::Error;

use crate::types::{SuggestionsFunction, DispatchFunction};

use super::parsers::MinecraftParser;

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

pub struct MinecraftRootDispatchNode<T> {
    pub literals: HashMap<&'static str, MinecraftDispatchNode<T>>,
    pub aliases: HashMap<&'static str, &'static str>,
}

impl <T> Clone for MinecraftRootDispatchNode<T> {
    fn clone(&self) -> Self {
        Self { literals: self.literals.clone(), aliases: self.aliases.clone() }
    }
}

impl <T> MinecraftRootDispatchNode<T> {
    pub fn new() -> Self {
        Self {
            literals: HashMap::new(),
            aliases: HashMap::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn merge_named(
        &mut self,
        dispatch: MinecraftDispatchNode<T>,
        name: &'static str,
        aliases: Vec<&'static str>,
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

    pub fn merge(&mut self, other: MinecraftRootDispatchNode<T>) -> result::Result<(), MergeError> {
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
        } else if !other.aliases.is_empty() {
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

pub struct MinecraftDispatchNode<T> {
    pub literals: BTreeMap<&'static str, MinecraftDispatchNode<T>>,
    pub aliases: BTreeMap<&'static str, &'static str>,
    pub parsers: Vec<MinecraftArgumentNode<T>>,
    pub executor: Option<DispatchFunction<T>>,
}

impl <T> Clone for MinecraftDispatchNode<T> {
    fn clone(&self) -> Self {
        Self { literals: self.literals.clone(), aliases: self.aliases.clone(), parsers: self.parsers.clone(), executor: self.executor.clone() }
    }
}

impl <T> MinecraftDispatchNode<T> {
    pub(crate) fn merge(&mut self, node: MinecraftDispatchNode<T>) -> result::Result<(), MergeError> {
        // Try to merge executor
        if self.executor.is_none() {
            self.executor = node.executor;
        } else if node.executor.is_some() {
            // Both self.executor and node.executor exist
            return Err(MergeError::DuplicateExecutor);
        }

        // Merge the parsers
        'merge: for new_parser in node.parsers {
            for parser in &mut self.parsers {
                if parser.parse == new_parser.parse {
                    parser.dispatch_node.merge(*new_parser.dispatch_node)?;
                    continue 'merge;
                }
            }
            self.parsers.push(new_parser);
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
        } else if !node.aliases.is_empty() {
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

pub struct MinecraftArgumentNode<T> {
    pub name: &'static str,
    pub parse: MinecraftParser,
    pub dispatch_node: Box<MinecraftDispatchNode<T>>,
    pub suggestion_type: Option<SuggestionType>,
    pub suggestions: Option<SuggestionsFunction>
}

impl <T> Clone for MinecraftArgumentNode<T> {
    fn clone(&self) -> Self {
        Self { name: self.name, parse: self.parse.clone(), dispatch_node: self.dispatch_node.clone(), suggestion_type: self.suggestion_type.clone(), suggestions: self.suggestions.clone() }
    }
}

// Tests

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use crate::minecraft::MinecraftParser;

    use super::{MinecraftArgumentNode, MinecraftDispatchNode, MinecraftRootDispatchNode};

    fn empty_root() -> MinecraftRootDispatchNode<()> {
        MinecraftRootDispatchNode {
            literals: HashMap::new(),
            aliases: HashMap::new(),
        }
    }

    fn empty_dispatch_node() -> MinecraftDispatchNode<()> {
        MinecraftDispatchNode {
            literals: BTreeMap::new(),
            aliases: BTreeMap::new(),
            parsers: vec![],
            executor: None,
        }
    }

    fn dispatch_node_with_numeric_parser<'a>(
        dispatch: MinecraftDispatchNode<()>,
    ) -> MinecraftDispatchNode<()> {
        let numeric_parser = MinecraftArgumentNode {
            name: "argument",
            parse: MinecraftParser::U8 {
                min: u8::MIN,
                max: u8::MAX,
            },
            dispatch_node: Box::from(dispatch),
            suggestion_type: None,
            suggestions: None
        };
        MinecraftDispatchNode {
            literals: BTreeMap::new(),
            aliases: BTreeMap::new(),
            parsers: vec![numeric_parser],
            executor: None,
        }
    }

    fn dispatch_node_with_string_parser(dispatch: MinecraftDispatchNode<()>) -> MinecraftDispatchNode<()> {
        let string_parser = MinecraftArgumentNode {
            name: "argument",
            parse: MinecraftParser::Word,
            dispatch_node: Box::from(dispatch),
            suggestion_type: None,
            suggestions: None
        };
        MinecraftDispatchNode {
            literals: BTreeMap::new(),
            aliases: BTreeMap::new(),
            parsers: vec![string_parser],
            executor: None,
        }
    }

    fn dispatch_node_with_executor() -> MinecraftDispatchNode<()> {
        use crate::types::{CommandDispatchResult, Span};
        fn hello(_context: &mut (), _: &[u8], _: &[Span]) -> CommandDispatchResult {
            CommandDispatchResult::Success(Ok(()))
        }

        MinecraftDispatchNode {
            literals: BTreeMap::new(),
            aliases: BTreeMap::new(),
            parsers: vec![],
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
        assert_eq!(
            root.merge_named(dispatch, "bye", vec!["bye1", "bye2"]),
            Ok(())
        );

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

        assert_eq!(literal.1.parsers.len(), 2);
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
}
