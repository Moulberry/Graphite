use std::collections::{HashMap, BTreeMap};

use protocol::{play::server, types::CommandNode};

use crate::dispatcher::{ArgumentNode, DispatchNode, RootDispatchNode};

use super::{
    parsers::MinecraftParser,
    proto_nodes::{MinecraftArgumentNode, MinecraftDispatchNode, MinecraftRootDispatchNode},
};

pub fn create_dispatcher_and_brigadier_packet(
    root: MinecraftRootDispatchNode,
) -> (RootDispatchNode, server::Commands) {
    let mut command_nodes = Vec::new();

    let literal_count = root.literals.len();
    let aliases_count = root.aliases.len();

    let mut children: Vec<i32> = Vec::with_capacity(literal_count + aliases_count + 2);
    let mut graphite_alias_map = HashMap::new();

    // Process literals
    let mut literals = HashMap::new();
    for (literal_name, literal_node) in &root.literals {
        let child = process_named_dispatch_node(literal_name, literal_node, &mut command_nodes);
        let (child_dispatch_node, child_command_node) = child;

        // Insert dispatch node (graphite)
        literals.insert(*literal_name, child_dispatch_node);

        // Push graphite -> brigadier mapping for aliases
        graphite_alias_map.insert(literal_name, child_command_node.clone());

        // Push command node (brigadier)
        let brigadier_index = command_nodes.len() as i32;
        children.push(brigadier_index);
        command_nodes.push(child_command_node);
    }

    // Process aliases
    for (alias_from, alias_to) in &root.aliases {
        // Get brigadier index
        let alias_for = graphite_alias_map.remove(alias_to).unwrap();

        match alias_for {
            // todo: use a redirect instead of cloning the node
            // for some reason redirect was causing the client to 
            // reject the brigadier packet
            CommandNode::Literal { children: node_children, is_executable, redirect, name: _ } => {
                // Create alias node (brigadier)
                let command_node = CommandNode::Literal {
                    children: node_children,
                    is_executable,
                    redirect,
                    name: alias_from,
                };

                // Push alias node (brigadier)
                let brigadier_index = command_nodes.len() as i32;
                children.push(brigadier_index);
                command_nodes.push(command_node);
            },
            _ => unreachable!()
        }        
    }

    // Create root dispatch node (graphite)
    let root_dispatch_node = RootDispatchNode {
        literals,
        aliases: root.aliases,
    };

    // Create root command node (brigadier)
    let root_command_node = CommandNode::Root { children };
    let root_index = command_nodes.len() as i32;
    command_nodes.push(root_command_node);

    // Create brigadier packet
    let brigadier_packet = server::Commands {
        nodes: command_nodes,
        root_index,
    };

    // Return values
    (root_dispatch_node, brigadier_packet)
}

fn process_named_dispatch_node(
    name: &'static str,
    dispatch: &MinecraftDispatchNode,
    command_nodes: &mut Vec<CommandNode>,
) -> (DispatchNode, CommandNode) {
    let is_executable = dispatch.executor.is_some();
    let (dispatch_node, brig_children) = process_dispatch_node(dispatch, command_nodes);

    let command_node = CommandNode::Literal {
        children: brig_children,
        is_executable,
        redirect: None,
        name,
    };

    (dispatch_node, command_node)
}

fn process_dispatch_node(
    dispatch: &MinecraftDispatchNode,
    command_nodes: &mut Vec<CommandNode>,
) -> (DispatchNode, Vec<i32>) {
    let literal_count = dispatch.literals.len();
    let aliases_count = dispatch.aliases.len();

    let mut children: Vec<i32> = Vec::with_capacity(literal_count + aliases_count + 2);
    let mut graphite_alias_map = HashMap::new();

    // Process literals
    let mut literals = BTreeMap::new();
    for (literal_name, literal_node) in &dispatch.literals {
        let child = process_named_dispatch_node(literal_name, literal_node, command_nodes);
        let (child_dispatch_node, child_command_node) = child;

        // Insert dispatch node (graphite)
        literals.insert(*literal_name, child_dispatch_node);

        // Push graphite -> brigadier mapping for aliases
        graphite_alias_map.insert(literal_name, child_command_node.clone());

        // Push command node (brigadier)
        let brigadier_index = command_nodes.len() as i32;
        children.push(brigadier_index);
        command_nodes.push(child_command_node);
    }

    // Process aliases
    for (alias_from, alias_to) in &dispatch.aliases {
        // Get brigadier index
        let alias_for = graphite_alias_map.remove(alias_to).unwrap();

        match alias_for {
            // todo: use a redirect instead of cloning the node
            // for some reason redirect was causing the client to 
            // reject the brigadier packet
            CommandNode::Literal { children: node_children, is_executable, redirect, name: _ } => {
                // Create alias node (brigadier)
                let command_node = CommandNode::Literal {
                    children: node_children,
                    is_executable,
                    redirect,
                    name: alias_from,
                };

                // Push alias node (brigadier)
                let brigadier_index = command_nodes.len() as i32;
                children.push(brigadier_index);
                command_nodes.push(command_node);
            },
            _ => unreachable!()
        }  
    }

    let mut parsers = Vec::new();

    // Process numeric parser
    if let Some(numeric_parser) = dispatch.numeric_parser.as_ref() {
        let argument = process_argument_node(numeric_parser, command_nodes);
        let (argument_node, command_node) = argument;

        // Insert dispatch node (graphite)
        parsers.push(argument_node);

        // Push command node (brigadier)
        let brigadier_index = command_nodes.len() as i32;
        children.push(brigadier_index);
        command_nodes.push(command_node);
    }

    // Process string parser
    if let Some(string_parser) = dispatch.string_parser.as_ref() {
        let argument = process_argument_node(string_parser, command_nodes);
        let (argument_node, command_node) = argument;

        // Insert dispatch node (graphite)
        parsers.push(argument_node);

        // Push command node (brigadier)
        let brigadier_index = command_nodes.len() as i32;
        children.push(brigadier_index);
        command_nodes.push(command_node);
    }

    let dispatch_node = DispatchNode {
        literals,
        aliases: dispatch.aliases.clone(),
        parsers,
        executor: dispatch.executor,
    };

    (dispatch_node, children)
}

fn process_argument_node<T: MinecraftParser>(
    argument: &MinecraftArgumentNode<T>,
    command_nodes: &mut Vec<CommandNode>,
) -> (ArgumentNode, CommandNode) {
    let (dispatch_node, brig_children) =
        process_dispatch_node(&argument.dispatch_node, command_nodes);

    let argument_node = ArgumentNode {
        parse: argument.parse.get_parse_func(),
        dispatch_node,
    };

    let command_node = CommandNode::Argument {
        children: brig_children,
        is_executable: argument.dispatch_node.executor.is_some(),
        redirect: None,
        suggestion: None,
        name: argument.name,
        parser: argument.parse.get_brigadier_parser(),
    };

    (argument_node, command_node)
}
