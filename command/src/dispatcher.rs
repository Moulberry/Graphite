use std::collections::{HashMap, BTreeMap};

use crate::types::{ParseState, SpannedWord, CommandDispatchResult, CommandResult, CommandParseResult, Span};

// Node implemenatations

pub struct RootDispatchNode {
    pub(crate) literals: HashMap<&'static str, DispatchNode>,
    pub(crate) aliases: HashMap<&'static str, &'static str>,
}

impl RootDispatchNode {
    pub fn dispatch(&self, input: &str) -> CommandDispatchResult {
        let parse_state = ParseState::new(input);
        self.dispatch_parse_state(parse_state)
    }

    pub fn dispatch_with_context<T>(&self, input: &str, context: &T) -> CommandDispatchResult {
        let mut parse_state = ParseState::new(input);
        parse_state.push_ref(context, parse_state.full_span);
        self.dispatch_parse_state(parse_state)
    }

    fn dispatch_parse_state(&self, mut parse_state: ParseState) -> CommandDispatchResult {
        if let Some(spanned_word) = parse_state.pop_input() {
            if let Some(aliased) = self.aliases.get(spanned_word.word) {
                // Aliased literal

                let literal = self
                    .literals
                    .get(*aliased)
                    .expect("literal must exist if it has an alias");

                literal.dispatch(&mut parse_state)
            } else {
                // Non-aliased

                let literal = self.literals.get(spanned_word.word);

                if let Some(literal) = literal {
                    literal.dispatch(&mut parse_state)
                } else {
                    CommandDispatchResult::UnknownCommand
                }
            }
        } else {
            CommandDispatchResult::IncompleteCommand
        }
    }
}

pub(crate) struct DispatchNode {
    pub(crate) literals: BTreeMap<&'static str, DispatchNode>,
    pub(crate) aliases: BTreeMap<&'static str, &'static str>,
    pub(crate) parsers: Vec<ArgumentNode>,
    pub(crate) executor: Option<fn(&[u8], &[Span]) -> CommandDispatchResult>,
}

impl DispatchNode {
    fn dispatch(&self, remaining: &mut ParseState) -> CommandDispatchResult {
        if let Some(next_word) = remaining.pop_input() {
            // There is some input remaining

            if let Some(aliased) = self.aliases.get(next_word.word) {
                // Literal match via alias, dispatch to there
                let literal = self
                    .literals
                    .get(*aliased)
                    .expect("literal must exist if it has an alias");

                literal.dispatch(remaining)
            } else if let Some(literal) = self.literals.get(next_word.word) {
                // Literal match, dispatch to there
                literal.dispatch(remaining)
            } else {
                // No literal match, try to parse the input
                let mut result: Option<CommandDispatchResult> = None;
                for arg in &self.parsers {
                    let prev_cursor = remaining.cursor();

                    let parse_result = arg.parse(next_word, remaining);
                    match parse_result {
                        CommandDispatchResult::ParseError { span: _, errmsg: _, continue_parsing } => {
                            if continue_parsing {
                                if result.is_none() {
                                    result = Some(parse_result);
                                }
                            } else {
                                return parse_result;
                            }
                        },
                        _ => return parse_result
                    }

                    // Parse failed, try next parser
                    // Also debug assert that the cursor didn't change
                    debug_assert!(
                        remaining.cursor() == prev_cursor,
                        "cursor was updated by an argument node that failed"
                    );
                }
                match result {
                    Some(dispatch_result) => dispatch_result,
                    None => CommandDispatchResult::TooManyArguments,
                }
            }
        } else {
            // There is no input remaining, see if this node is an executor

            if let Some(executor) = self.executor {
                // This node is an executor, lets execute!
                executor(remaining.arguments.as_slice(), remaining.argument_spans.as_slice())
            } else {
                // Node isn't an executor, input *should* have had more remaining
                CommandDispatchResult::IncompleteCommand
            }
        }
    }
}

// Argument node

pub(crate) struct ArgumentNode {
    pub(crate) parse: fn(SpannedWord, &mut ParseState) -> CommandParseResult,
    pub(crate) dispatch_node: DispatchNode,
}

impl ArgumentNode {
    fn parse(&self, word: SpannedWord, remaining: &mut ParseState) -> CommandDispatchResult {
        // Try to parse a value
        let parse_result = (self.parse)(word, remaining);

        match parse_result {
            CommandParseResult::Ok => {
                // Parse succeeded, continue dispatching
                self.dispatch_node.dispatch(remaining)
            },
            CommandParseResult::Err { span, errmsg, continue_parsing } => {
                // Parse failed, bubble up ParseError
                CommandDispatchResult::ParseError { span, errmsg, continue_parsing }
            }
        }
    }
}