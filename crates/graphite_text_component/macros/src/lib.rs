

use proc_macro2::{Span, TokenTree};

struct MacroError(Span, String);

mod parse;
mod tree;

#[proc_macro]
pub fn component(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    match component_inner(input) {
        Ok(nodes) => nodes,
        Err(error) => {
            let string = format!("component: {}", error.1);
            quote::quote_spanned!(error.0 => compile_error!(#string)).into()
        }
    }
}

fn component_inner(input: proc_macro::TokenStream) -> Result<proc_macro::TokenStream, MacroError> {
    let input: proc_macro2::TokenStream = input.into();
    let tokens = input.into_iter().peekable();

    let nodes = tree::parse_all(tokens)?;
    println!("{:#?}", parse::parse(nodes)?);

    Ok(Default::default())
}
