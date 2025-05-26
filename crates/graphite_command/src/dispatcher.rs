use std::{borrow::Cow, collections::{BTreeMap, HashMap}};

use crate::types::{
    CommandDispatchResult, CommandParseResult, SuggestionsFunction, DispatchFunction, ParseState, SpannedWord, Suggestions
};

// Node implemenatations

pub struct RootDispatchNode<T> {
    pub(crate) literals: HashMap<&'static str, DispatchNode<T>>,
    pub(crate) aliases: HashMap<&'static str, &'static str>,
}

impl <T> RootDispatchNode<T> {
    pub fn dispatch(&self, context: &mut T, input: &str) -> CommandDispatchResult {
        let parse_state = ParseState::new(input);
        self.dispatch_with(context, parse_state)
    }

    pub fn dispatch_with(&self, context: &mut T, mut parse_state: ParseState) -> CommandDispatchResult {
        if let Some(spanned_word) = parse_state.pop_input() {
            if let Some(aliased) = self.aliases.get(spanned_word.word) {
                // Aliased literal

                let literal = self
                    .literals
                    .get(*aliased)
                    .expect("literal must exist if it has an alias");

                literal.dispatch(context, parse_state)
            } else {
                // Non-aliased

                let literal = self.literals.get(spanned_word.word);

                if let Some(literal) = literal {
                    literal.dispatch(context, parse_state)
                } else {
                    CommandDispatchResult::UnknownCommand {
                        span: spanned_word.span, 
                    }
                }
            }
        } else {
            CommandDispatchResult::IncompleteCommand
        }
    }

    pub fn suggest<'a>(&self, input: &'a str) -> Suggestions<'a> {
        let parse_state = ParseState::new(input);
        self.suggest_with(parse_state)
    }

    pub fn suggest_with<'a>(&self, mut parse_state: ParseState<'a>) -> Suggestions<'a> {
        if let Some(spanned_word) = parse_state.pop_input() {
            if let Some(aliased) = self.aliases.get(spanned_word.word) {
                // Aliased literal

                let literal = self
                    .literals
                    .get(*aliased)
                    .expect("literal must exist if it has an alias");

                literal.suggest(&mut parse_state)
            } else {
                // Non-aliased

                let literal = self.literals.get(spanned_word.word);

                if let Some(literal) = literal {
                    literal.suggest(&mut parse_state)
                } else {
                    return Suggestions {
                        start: spanned_word.span.start,
                        values: Vec::new(),
                    };
                }
            }
        } else {
            return Suggestions {
                start: parse_state.full_span.end,
                values: Vec::new(),
            }
        }
    }
}

pub(crate) struct DispatchNode<T> {
    pub(crate) literals: BTreeMap<&'static str, DispatchNode<T>>,
    pub(crate) aliases: BTreeMap<&'static str, &'static str>,
    pub(crate) parsers: Vec<ArgumentNode<T>>,
    pub(crate) executor: Option<DispatchFunction<T>>,
}

impl <T> DispatchNode<T> {
    fn dispatch(&self, context: &mut T, mut remaining: ParseState) -> CommandDispatchResult {
        if let Some(next_word) = remaining.pop_input() {
            // There is some input remaining

            if let Some(aliased) = self.aliases.get(next_word.word) {
                // Literal match via alias, dispatch to there
                let literal = self
                    .literals
                    .get(*aliased)
                    .expect("literal must exist if it has an alias");

                literal.dispatch(context, remaining)
            } else if let Some(literal) = self.literals.get(next_word.word) {
                // Literal match, dispatch to there
                literal.dispatch(context, remaining)
            } else {
                // No literal match, try to parse the input
                let mut result: Option<CommandDispatchResult> = None;
                for arg in &self.parsers {
                    let parse_result = arg.parse_and_dispatch(context, next_word, remaining.clone());

                    match parse_result {
                        CommandDispatchResult::ParseError {
                            span: _,
                            error: _,
                            continue_parsing,
                        } => {
                            if continue_parsing {
                                if result.is_none() {
                                    result = Some(parse_result);
                                }
                            } else {
                                return parse_result;
                            }
                        }
                        _ => return parse_result,
                    }
                }
                match result {
                    Some(dispatch_result) => dispatch_result,
                    None => CommandDispatchResult::TooManyArguments {
                        span: remaining.remaining_span()
                    },
                }
            }
        } else {
            // There is no input remaining, see if this node is an executor

            if let Some(executor) = self.executor {
                // This node is an executor, lets execute!
                let (arguments, spans) = remaining.get_arguments();
                executor(context, arguments, spans)
            } else {
                // Node isn't an executor, input *should* have had more remaining
                CommandDispatchResult::IncompleteCommand
            }
        }
    }

    fn suggest<'a>(&self, remaining: &mut ParseState<'a>) -> Suggestions<'a> {
        if let Some(next_word) = remaining.pop_input() {
            // There is some input remaining

            if let Some(aliased) = self.aliases.get(next_word.word) {
                // Literal match via alias, run suggestions there
                let literal = self
                    .literals
                    .get(*aliased)
                    .expect("literal must exist if it has an alias");

                literal.suggest(remaining)
            } else if let Some(literal) = self.literals.get(next_word.word) {
                // Literal match, run suggestions there
                literal.suggest(remaining)
            } else {
                // No literal match, try to parse the input
                for arg in &self.parsers {
                    let prev_cursor = remaining.cursor();

                    if let Some(suggestions) = arg.parse_and_suggest(next_word, remaining) {
                        return suggestions;
                    }

                    // Parse failed, try next parser
                    // Also debug assert that the cursor didn't change
                    debug_assert!(
                        remaining.cursor() == prev_cursor,
                        "cursor was updated by an argument node that failed"
                    );
                }
                return Suggestions {
                    start: next_word.span.start,
                    values: Vec::new()
                };
            }
        } else {
            // There is no input remaining, try to complete with empty string
            for arg in &self.parsers {
                if let Some(suggestions) = arg.suggestions {
                    return Suggestions {
                        start: remaining.original_input.len(),
                        values: (suggestions)("")
                    };
                }
            }

            return Suggestions {
                start: remaining.original_input.len(),
                values: Vec::new()
            };
        }
    }
}

// Argument node

pub(crate) struct ArgumentNode<T> {
    pub(crate) parse: Box<dyn Fn(SpannedWord, &mut ParseState) -> CommandParseResult>,
    pub(crate) suggestions: Option<SuggestionsFunction>,
    pub(crate) dispatch_node: DispatchNode<T>,
}

impl <T> ArgumentNode<T> {
    fn parse_and_dispatch(&self, context: &mut T, word: SpannedWord, mut remaining: ParseState) -> CommandDispatchResult {
        // Try to parse a value
        let parse_result = (self.parse)(word, &mut remaining);

        match parse_result {
            CommandParseResult::Ok => {
                // Parse succeeded, continue dispatching
                self.dispatch_node.dispatch(context, remaining)
            }
            CommandParseResult::Err {
                span,
                error,
                continue_parsing,
            } => {
                // Parse failed, bubble up ParseError
                CommandDispatchResult::ParseError {
                    span,
                    error,
                    continue_parsing,
                }
            }
        }
    }

    fn parse_and_suggest<'a>(&self, word: SpannedWord, remaining: &mut ParseState<'a>) -> Option<Suggestions<'a>> {
        // Try to parse a value
        let parse_result = (self.parse)(word, remaining);

        if remaining.is_finished() {
            let text = &remaining.original_input[word.span.start..];

            if let Some(suggestions) = self.suggestions {
                return Some(Suggestions {
                    start: word.span.start,
                    values: (suggestions)(text),
                });
            } else {
                return Some(Suggestions {
                    start: word.span.start,
                    values: Vec::new(),
                });
            }
        }

        match parse_result {
            CommandParseResult::Ok => {
                // Parse succeeded, continue finding completions
                Some(self.dispatch_node.suggest(remaining))
            }
            CommandParseResult::Err {
                span: _,
                error: _,
                continue_parsing,
            } => {
                if continue_parsing {
                    return None;
                } else {
                    return Some(Suggestions {
                        start: word.span.start,
                        values: Vec::new(),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use maplit::hashmap;

    use crate::dispatcher::{ArgumentNode, DispatchNode, RootDispatchNode};
    use crate::types::{ParseErrorType, ParseState};
    use crate::types::{CommandDispatchResult, CommandParseResult, Span, SpannedWord};

    #[test]
    pub fn dispatch_with_parse() {
        static mut DISPATCH_EXECUTED: bool = false;

        fn hello_world(_context: &mut (), data: &[u8], spans: &[Span]) -> CommandDispatchResult {
            #[repr(C)]
            struct Data(u8, &'static str, u16);

            debug_assert_eq!(spans.len(), 3);
            debug_assert_eq!(data.len(), std::mem::size_of::<Data>());
            let data: &Data = unsafe { &*(data as *const _ as *const Data) };

            assert_eq!(data.0, 100);
            assert_eq!(data.1, "my_string");
            assert_eq!(data.2, 8372);
            unsafe { DISPATCH_EXECUTED = true };

            CommandDispatchResult::Success(Ok(()))
        }

        let root = RootDispatchNode {
            literals: hashmap!(
                "hello" => DispatchNode {
                    literals: BTreeMap::new(),
                    aliases: BTreeMap::new(),
                    parsers: vec![
                        ArgumentNode {
                            parse: Box::new(parse_u8),
                            suggestions: None,
                            dispatch_node: DispatchNode {
                                literals: BTreeMap::new(),
                                aliases: BTreeMap::new(),
                                parsers: vec![
                                    ArgumentNode {
                                        parse: Box::new(parse_str),
                                        suggestions: None,
                                        dispatch_node: DispatchNode {
                                            literals: BTreeMap::new(),
                                            aliases: BTreeMap::new(),
                                            parsers: vec![
                                                ArgumentNode {
                                                    parse: Box::new(parse_u16),
                                                    suggestions: None,
                                                    dispatch_node: DispatchNode {
                                                        literals: BTreeMap::new(),
                                                        aliases: BTreeMap::new(),
                                                        parsers: vec![],
                                                        executor: Some(hello_world)
                                                    }
                                                }
                                            ],
                                            executor: None,
                                        }
                                    }
                                ],
                                executor: None,
                            }
                        }
                    ],
                    executor: None,
                }
            ),
            aliases: HashMap::new(),
        };

        root.dispatch(&mut (), "hello 100 my_string 8372");

        assert!(unsafe { DISPATCH_EXECUTED });
    }

    #[test]
    pub fn dispatch_with_context() {
        static mut DISPATCH_EXECUTED: bool = false;

        struct MyStruct(u32);

        fn my_command(_context: &mut (), data: &[u8], spans: &[Span]) -> CommandDispatchResult {
            #[repr(C)]
            struct Data(&'static MyStruct);

            debug_assert_eq!(spans.len(), 1);
            debug_assert_eq!(data.len(), std::mem::size_of::<Data>());
            let data: &Data = unsafe { &*(data as *const _ as *const Data) };

            assert_eq!(data.0 .0, 873183);
            unsafe { DISPATCH_EXECUTED = true };

            CommandDispatchResult::Success(Ok(()))
        }

        let root = RootDispatchNode {
            literals: hashmap!(
                "execute" => DispatchNode {
                    literals: BTreeMap::new(),
                    aliases: BTreeMap::new(),
                    parsers: vec![],
                    executor: Some(my_command)
                }
            ),
            aliases: HashMap::new(),
        };

        let my_struct = MyStruct(873183);
        let mut parse_state = ParseState::new("execute");
        parse_state.push_ref(&my_struct, parse_state.full_span);
        root.dispatch_with(&mut (), parse_state);

        assert!(unsafe { DISPATCH_EXECUTED });
    }

    // Parser functions

    fn parse_u8(input: SpannedWord, state: &mut ParseState) -> CommandParseResult {
        match input.word.parse::<u8>() {
            Ok(parsed) => {
                state.push_arg(parsed, input.span);
                CommandParseResult::Ok
            }
            Err(_) => CommandParseResult::Err {
                span: input.span,
                error: ParseErrorType::Other("failed to parse u8".into()),
                continue_parsing: true,
            },
        }
    }

    fn parse_u16(input: SpannedWord, state: &mut ParseState) -> CommandParseResult {
        match input.word.parse::<u16>() {
            Ok(parsed) => {
                state.push_arg(parsed, input.span);
                CommandParseResult::Ok
            }
            Err(_) => CommandParseResult::Err {
                span: input.span,
                error: ParseErrorType::Other("failed to parse u8".into()),
                continue_parsing: true,
            },
        }
    }

    fn parse_str(input: SpannedWord, state: &mut ParseState) -> CommandParseResult {
        state.push_str(input.word, input.span);
        CommandParseResult::Ok
    }
}
