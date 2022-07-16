use std::collections::{HashMap, BTreeMap};

use maplit::hashmap;

use crate::dispatcher::{ArgumentNode, DispatchNode, ParseState, RootDispatchNode};

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

fn parse_u8(input: &str, state: &mut ParseState) -> anyhow::Result<()> {
    let parsed: u8 = input.parse()?;
    state.push_arg(parsed);
    Ok(())
}

fn parse_u16(input: &str, state: &mut ParseState) -> anyhow::Result<()> {
    let parsed: u16 = input.parse()?;
    state.push_arg(parsed);
    Ok(())
}

fn parse_str(input: &str, state: &mut ParseState) -> anyhow::Result<()> {
    state.push_str(input);
    Ok(())
}
