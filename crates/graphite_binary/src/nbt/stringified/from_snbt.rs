use std::str::FromStr;

use anyhow::bail;

use crate::nbt::*;

pub fn from_snbt(mut snbt: &str) -> anyhow::Result<NBT> {
    let mut nodes = Vec::new();

    // todo: check if using peekable gives perf
    // let snbt = snbt.chars().peekable();

    // Make sure snbt starts with an opening brace
    let next_char = peek_non_whitespace(&mut snbt)?;
    if next_char == '{' {
        snbt = &snbt[1..];
    } else {
        bail!("from_snbt: snbt must start with opening brace ({{)")
    }

    // Parse the root compound
    let children = read_compound(&mut snbt, &mut nodes)?;

    // Make sure there is no more input
    for c in snbt.chars() {
        if !c.is_whitespace() {
            bail!("from_snbt: expected end of input")
        }
    }

    Ok(NBT {
        root_name: String::new(),
        root_children: children,
        nodes,
    })
}

fn read_node(snbt: &mut &str, nodes: &mut Vec<NBTNode>) -> anyhow::Result<(usize, TagType)> {
    let (node, type_id) = match peek_non_whitespace(snbt)? {
        '0'..='9' | '.' | '-' => read_numeric_node(snbt)?,
        '{' => {
            *snbt = &snbt[1..];
            (
                NBTNode::Compound(read_compound(snbt, nodes)?),
                TAG_COMPOUND_ID,
            )
        }
        '[' => {
            *snbt = &snbt[1..];
            read_array_node(snbt, nodes)?
        }
        '"' => (NBTNode::String(read_string(snbt)?), TAG_STRING_ID),
        't' => {
            if snbt.len() >= 4 && &snbt[..4] == "true" {
                (NBTNode::Byte(1), TAG_BYTE_ID)
            } else {
                bail!("unknown start of type: t");
            }
        }
        'f' => {
            if snbt.len() >= 5 && &snbt[..5] == "false" {
                (NBTNode::Byte(0), TAG_BYTE_ID)
            } else {
                bail!("unknown start of type: f");
            }
        }
        c => bail!("unknown start of type: {}", c),
    };

    nodes.push(node);
    Ok((nodes.len() - 1, type_id))
}

fn peek_non_whitespace(snbt: &mut &str) -> anyhow::Result<char> {
    for (index, c) in snbt.char_indices() {
        if !c.is_whitespace() {
            // Skip the whitespace
            // snbt[0] will be c
            *snbt = &snbt[index..];
            return Ok(c);
        }
    }
    bail!("next_char: unexpected end of input");
}

fn read_compound(snbt: &mut &str, nodes: &mut Vec<NBTNode>) -> anyhow::Result<NBTCompound> {
    let mut children = NBTCompound(Vec::new());

    // Special case for empty compound `{}`
    let next_char = peek_non_whitespace(snbt)?;
    if next_char == '}' {
        *snbt = &snbt[1..];
        return Ok(children);
    }

    loop {
        let name = read_key(snbt)?;

        if peek_non_whitespace(snbt)? == ':' {
            *snbt = &snbt[1..];
        } else {
            bail!("read_compound: key must be followed by a colon (:)")
        }

        let (idx, _type_id) = read_node(snbt, nodes)?;

        match children.binary_search(name.as_ref()) {
            Ok(_) => bail!("read_compound: duplicate key"),
            Err(index) => {
                children.0.insert(index, (name.into(), idx));
            }
        }

        match peek_non_whitespace(snbt)? {
            '}' => {
                *snbt = &snbt[1..];
                return Ok(children);
            }
            ',' => *snbt = &snbt[1..],
            c => bail!("read_compound: unknown continuation: {}", c),
        }
    }
}

fn read_key(snbt: &mut &str) -> anyhow::Result<String> {
    let first_char = peek_non_whitespace(snbt)?;

    if first_char == '"' {
        read_string(snbt)
    } else {
        for (index, c) in snbt.char_indices() {
            match c {
                '0'..='9' | 'A'..='Z' | 'a'..='z' | '.' | '_' | '+' | '-' => continue,
                ':' => {
                    let string = snbt[..index].into();
                    *snbt = &snbt[index..];
                    return Ok(string);
                }
                c => bail!("read_key: invalid character: {}", c),
            }
        }
        bail!("read_key: unexpected end of input");
    }
}

fn read_string(snbt: &mut &str) -> anyhow::Result<String> {
    let first_char = peek_non_whitespace(snbt)?;

    if first_char != '"' {
        bail!("read_string: first character must be quote literal (\")");
    } else {
        *snbt = &snbt[1..];

        let mut string = String::new();
        let mut start = 0;
        let mut escaping = false;

        for (index, c) in snbt.char_indices() {
            match c {
                '\\' => {
                    if escaping {
                        escaping = false;
                    } else {
                        string.push_str(&snbt[start..index]);
                        start = index + 1;
                    }
                }
                '"' => {
                    if escaping {
                        escaping = false;
                    } else {
                        string.push_str(&snbt[start..index]);
                        *snbt = &snbt[(index + 1)..];
                        return Ok(string);
                    }
                }
                c => {
                    if escaping {
                        bail!("read_string: unknown escape sequence: \\{}", c);
                    } else {
                        continue;
                    }
                }
            }
        }
        bail!("read_string: unexpected end of input");
    }
}

fn read_numeric_node(snbt: &mut &str) -> anyhow::Result<(NBTNode, TagType)> {
    let mut has_decimal = false;
    for (index, c) in snbt.char_indices() {
        match c {
            '-' => {
                if index != 0 {
                    bail!("read_numeric_node: minus literal (-) is only valid at the beginning")
                }
            }
            '0'..='9' => {
                continue;
            }
            '.' => {
                if has_decimal {
                    bail!("read_numeric_node: found multiple decimal points while parsing number");
                } else {
                    has_decimal = true;
                }
                continue;
            }
            'b' | 'B' => {
                let number_string = &snbt[..index];
                *snbt = &snbt[(index + 1)..];
                return Ok((NBTNode::Byte(number_string.parse()?), TAG_BYTE_ID));
            }
            's' | 'S' => {
                let number_string = &snbt[..index];
                *snbt = &snbt[(index + 1)..];
                return Ok((NBTNode::Short(number_string.parse()?), TAG_SHORT_ID));
            }
            'l' | 'L' => {
                let number_string = &snbt[..index];
                *snbt = &snbt[(index + 1)..];
                return Ok((NBTNode::Long(number_string.parse()?), TAG_LONG_ID));
            }
            'f' | 'F' => {
                let number_string = &snbt[..index];
                *snbt = &snbt[(index + 1)..];
                return Ok((NBTNode::Float(number_string.parse()?), TAG_FLOAT_ID));
            }
            'd' | 'D' => {
                let number_string = &snbt[..index];
                *snbt = &snbt[(index + 1)..];
                return Ok((NBTNode::Double(number_string.parse()?), TAG_DOUBLE_ID));
            }
            _ => {
                let number_string = &snbt[..index];
                *snbt = &snbt[index..];
                if has_decimal {
                    return Ok((NBTNode::Double(number_string.parse()?), TAG_DOUBLE_ID));
                } else {
                    return Ok((NBTNode::Int(number_string.parse()?), TAG_INT_ID));
                }
            }
        }
    }
    bail!("read_numeric_node: unexpected end of input");
}

enum PrimArrParseState {
    WaitingForNumber,
    WaitingForComma,
    InNumber { start: usize },
}

fn read_array_node(snbt: &mut &str, nodes: &mut Vec<NBTNode>) -> anyhow::Result<(NBTNode, TagType)> {
    let next_char = peek_non_whitespace(snbt)?;
    match next_char {
        // Primitive ByteArray
        'B' => {
            *snbt = &snbt[1..];
            match peek_non_whitespace(snbt)? {
                ';' => *snbt = &snbt[1..],
                _ => bail!("read_array_node: expect semicolon (;) after B"),
            }

            Ok((
                NBTNode::ByteArray(read_primitive_array(snbt)?),
                TAG_BYTE_ARRAY_ID,
            ))
        }
        // Primitive IntArray
        'I' => {
            *snbt = &snbt[1..];
            match peek_non_whitespace(snbt)? {
                ';' => *snbt = &snbt[1..],
                _ => bail!("read_array_node: expect semicolon (;) after I"),
            }

            Ok((
                NBTNode::IntArray(read_primitive_array(snbt)?),
                TAG_INT_ARRAY_ID,
            ))
        }
        // Primitive LongArray
        'L' => {
            *snbt = &snbt[1..];
            match peek_non_whitespace(snbt)? {
                ';' => *snbt = &snbt[1..],
                _ => bail!("read_array_node: expect semicolon (;) after L"),
            }

            Ok((
                NBTNode::LongArray(read_primitive_array(snbt)?),
                TAG_LONG_ARRAY_ID,
            ))
        }
        // Special case for empty list `[]`
        ']' => {
            *snbt = &snbt[1..];
            Ok((
                NBTNode::List {
                    type_id: TAG_END_ID,
                    children: Vec::new(),
                },
                TAG_LIST_ID,
            ))
        }
        // Normal list
        _ => {
            let mut children = Vec::new();

            let (idx, first_type_id) = read_node(snbt, nodes)?;
            children.push(idx);

            loop {
                match peek_non_whitespace(snbt)? {
                    ']' => {
                        *snbt = &snbt[1..];
                        return Ok((
                            NBTNode::List {
                                type_id: first_type_id,
                                children,
                            },
                            TAG_LIST_ID,
                        ));
                    }
                    ',' => *snbt = &snbt[1..],
                    c => bail!("read_array_node: unknown continuation: {}", c),
                }

                let (idx, type_id) = read_node(snbt, nodes)?;
                children.push(idx);

                if type_id != first_type_id {
                    bail!("read_array_node: elements in array have different type")
                }
            }
        }
    }
}

fn read_primitive_array<T: FromStr>(snbt: &mut &str) -> anyhow::Result<Vec<T>> {
    let mut values = Vec::new();
    let mut state = PrimArrParseState::WaitingForNumber;
    for (index, c) in snbt.char_indices() {
        match c {
            ']' => {
                match state {
                    PrimArrParseState::WaitingForComma => (),
                    PrimArrParseState::WaitingForNumber => {
                        bail!("read_primitive_array: expected numeric character, got ]")
                    }
                    PrimArrParseState::InNumber { start } => {
                        let value: T = snbt[start..index].parse().map_err(|_| {
                            anyhow::anyhow!("read_primitive_array: failed to parse")
                        })?;
                        values.push(value);
                    }
                }

                *snbt = &snbt[(index + 1)..];
                return Ok(values);
            }
            '0'..='9' | '-' => match state {
                PrimArrParseState::WaitingForNumber => {
                    state = PrimArrParseState::InNumber { start: index }
                }
                PrimArrParseState::InNumber { start: _ } => continue,
                PrimArrParseState::WaitingForComma => {
                    bail!("read_primitive_array: expected comma, got numeric character")
                }
            },
            ',' => {
                match state {
                    PrimArrParseState::WaitingForComma => (),
                    PrimArrParseState::WaitingForNumber => {
                        bail!("read_primitive_array: expected numeric character, got comma")
                    }
                    PrimArrParseState::InNumber { start } => {
                        let value: T = snbt[start..index].parse().map_err(|_| {
                            anyhow::anyhow!("read_primitive_array: failed to parse")
                        })?;
                        values.push(value);
                    }
                }
                state = PrimArrParseState::WaitingForNumber;
            }
            ' ' => continue,
            c => {
                // todo: this is very permissive
                // this should only allow b/B (for byte arrays), l/L (for long arrays) and nothing for int arrays
                match state {
                    PrimArrParseState::WaitingForComma => {
                        bail!("read_primitive_array: expected comma, got `{}`", c)
                    }
                    PrimArrParseState::WaitingForNumber => bail!(
                        "read_primitive_array: expected numeric character, got `{}`",
                        c
                    ),
                    PrimArrParseState::InNumber { start } => {
                        let value: T = snbt[start..index].parse().map_err(|_| {
                            anyhow::anyhow!("read_primitive_array: failed to parse")
                        })?;
                        values.push(value);
                    }
                }
                state = PrimArrParseState::WaitingForComma;
            }
        }
    }
    bail!("read_array_node: unexpected end of input");
}
