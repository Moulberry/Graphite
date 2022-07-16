use std::collections::{HashMap, BTreeMap};

use command_derive::brigadier;

use crate::minecraft::MinecraftArgumentNode;

pub mod dispatcher;
pub mod minecraft;

#[cfg(test)]
mod command_tests;

fn main2() {
    /*let mut root = minecraft::MinecraftRootDispatchNode {
        literals: HashMap::new(),
        aliases: HashMap::new(),
    };

    fn my_func(input: &[u8]) {
        println!("executed my function!");
        println!("had data: {:?}", input);
    }

    let dispatch = minecraft::MinecraftDispatchNode {
        literals: BTreeMap::new(),
        aliases: BTreeMap::new(),
        numeric_parser: Some(MinecraftArgumentNode {
            name: "number",
            parse: minecraft::NumericParser::U8,
            dispatch_node: Box::from(minecraft::MinecraftDispatchNode {
                literals: BTreeMap::new(),
                aliases: BTreeMap::new(),
                numeric_parser: None,
                string_parser: None,
                executor: Some(my_func),
            }),
        }),
        string_parser: None,
        executor: None,
    };

    root.merge(dispatch, "hello", vec![]).unwrap();

    let processed = minecraft::create_dispatcher_and_brigadier_packet(root);
    let (dispatcher, commands) = processed;*/

    //println!("Brigadier Commands Packet: {:?}", commands);
    //dispatcher.dispatch("hello 100");
}


//#[brigadier_autoregister]
fn main() {
    
    #[brigadier("hello", {})]
    fn my_function(number: u8) {
        println!("number: {}", number);
    }

    let (dispatcher, packet) = minecraft::create_dispatcher_and_brigadier_packet(my_function);

    println!("{:?}", packet);
    dispatcher.dispatch("hello 10") // outputs: "number: 10"

    /*#[brigadier("hello {} subcommand {}")]
    fn my_function2(number: u8, my_param: u8) {
        println!("number: {}", number);
    }

    #[brigadier("hello {complete=my_complete_function}")]
    fn my_function3(string: &str) {
        println!("string: {}", string);
    }*/
}

/*
fn register2() {
    fn my_function__parse(data: &[u8]) {
        #[repr(C)]
        struct Data(u8);

        debug_assert_eq!(data.len(), std::mem::size_of::<Data>());
        let data: &Data = unsafe { std::mem::transmute(data as *const _ as *const ()) };

        my_function(data.0);
    }

    let my_function: minecraft::MinecraftDispatchNode = minecraft::MinecraftDispatchNode {
        literals: BTreeMap::new(),
        aliases: BTreeMap::new(),
        numeric_parser: Some(MinecraftArgumentNode {
            name: "number",
            parse: minecraft::NumericParser::U8,
            dispatch_node: Box::from(minecraft::MinecraftDispatchNode {
                literals: BTreeMap::new(),
                aliases: BTreeMap::new(),
                numeric_parser: None,
                string_parser: None,
                executor: Some(my_function__parse),
            }),
        }),
        string_parser: None,
        executor: None,
    };

    fn my_function(number: u8) {
        println!("number: {}", number);
    }
}
*/