use std::collections::{HashMap, BTreeMap};

use maplit::hashmap;

use crate::dispatcher::{ArgumentNode, DispatchNode, RootDispatchNode};
use crate::types::{SpannedWord, CommandParseResult};
use crate::types::ParseState;

#[test]
pub fn dispatch_with_parse() {
    static mut DISPATCH_EXECUTED: bool = false;

    fn hello_world(input: *const ()) {
        #[repr(C)]
        struct HelloWorldData(u8, &'static str, u16);

        let input: &HelloWorldData = unsafe { std::mem::transmute(input) };

        assert_eq!(input.0, 100);
        assert_eq!(input.1, "my_string");
        assert_eq!(input.2, 8372);
        unsafe { DISPATCH_EXECUTED = true };
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
                                                    executor: Some(unsafe { std::mem::transmute(hello_world as *const ()) })
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

    fn my_command(input: *const ()) {
        #[repr(C)]
        struct Data(&'static MyStruct);

        let input: &Data = unsafe { std::mem::transmute(input) };

        assert_eq!(input.0 .0, 873183);
        unsafe { DISPATCH_EXECUTED = true };
    }

    let root = RootDispatchNode {
        literals: hashmap!(
            "execute" => DispatchNode {
                literals: BTreeMap::new(),
                aliases: BTreeMap::new(),
                parsers: vec![],
                executor: Some(unsafe { std::mem::transmute(my_command as *const ()) })
            }
        ),
        aliases: HashMap::new(),
    };

    let my_struct = MyStruct(873183);
    root.dispatch_with_context("execute", &my_struct);

    assert!(unsafe { DISPATCH_EXECUTED });
}

// Parser functions

fn parse_u8(input: SpannedWord, state: &mut ParseState) -> CommandParseResult {
    match input.word.parse::<u8>() {
        Ok(parsed) => {
            state.push_arg(parsed, input.span);
            CommandParseResult::Ok
        },
        Err(_) => CommandParseResult::Err { span: input.span, errmsg: "failed to parse u8".into(), continue_parsing: true }
    }
}

fn parse_u16(input: SpannedWord, state: &mut ParseState) -> CommandParseResult {
    match input.word.parse::<u16>() {
        Ok(parsed) => {
            state.push_arg(parsed, input.span);
            CommandParseResult::Ok
        },
        Err(_) => CommandParseResult::Err { span: input.span, errmsg: "failed to parse u8".into(), continue_parsing: true }
    }
}

fn parse_str(input: SpannedWord, state: &mut ParseState) -> CommandParseResult {
    state.push_str(input.word, input.span);
    CommandParseResult::Ok
}
