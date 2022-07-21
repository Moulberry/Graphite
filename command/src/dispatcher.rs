use std::collections::{BTreeMap, HashMap};

use bytemuck::NoUninit;

use crate::types::{
    CommandDispatchResult, CommandParseResult, DispatchFunction, ParseState, SpannedWord,
};

// Node implemenatations

pub struct RootDispatchNode {
    pub(crate) literals: HashMap<&'static str, DispatchNode>,
    pub(crate) aliases: HashMap<&'static str, &'static str>,
}

impl RootDispatchNode {
    pub fn dispatch(&self, input: &str) -> CommandDispatchResult {
        let parse_state = ParseState::new(input);
        self.dispatch_with(parse_state)
    }

    pub fn dispatch_with(&self, mut parse_state: ParseState) -> CommandDispatchResult {
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
    pub(crate) executor: Option<DispatchFunction>,
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
                        CommandDispatchResult::ParseError {
                            span: _,
                            errmsg: _,
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
                let (arguments, spans) = remaining.get_arguments();
                executor(arguments, spans)
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
            }
            CommandParseResult::Err {
                span,
                errmsg,
                continue_parsing,
            } => {
                // Parse failed, bubble up ParseError
                CommandDispatchResult::ParseError {
                    span,
                    errmsg,
                    continue_parsing,
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
    use crate::types::ParseState;
    use crate::types::{CommandDispatchResult, CommandParseResult, Span, SpannedWord};

    #[test]
    pub fn dispatch_with_parse() {
        static mut DISPATCH_EXECUTED: bool = false;

        fn hello_world(data: &[u8], spans: &[Span]) -> CommandDispatchResult {
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
                            parse: parse_u8,
                            dispatch_node: DispatchNode {
                                literals: BTreeMap::new(),
                                aliases: BTreeMap::new(),
                                parsers: vec![
                                    ArgumentNode {
                                        parse: parse_str,
                                        dispatch_node: DispatchNode {
                                            literals: BTreeMap::new(),
                                            aliases: BTreeMap::new(),
                                            parsers: vec![
                                                ArgumentNode {
                                                    parse: parse_u16,
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

        root.dispatch("hello 100 my_string 8372");

        assert!(unsafe { DISPATCH_EXECUTED });
    }

    #[test]
    pub fn dispatch_with_context() {
        static mut DISPATCH_EXECUTED: bool = false;

        struct MyStruct(u32);

        fn my_command(data: &[u8], spans: &[Span]) -> CommandDispatchResult {
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
        root.dispatch_with(parse_state);

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
                errmsg: "failed to parse u8".into(),
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
                errmsg: "failed to parse u8".into(),
                continue_parsing: true,
            },
        }
    }

    fn parse_str(input: SpannedWord, state: &mut ParseState) -> CommandParseResult {
        state.push_str(input.word, input.span);
        CommandParseResult::Ok
    }
}
