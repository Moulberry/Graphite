use proc_macro::TokenStream;
use quote::quote_spanned;
use syn::{parse_macro_input, Type};

mod universe;
mod world;

#[proc_macro_derive(UniverseTicker)]
pub fn derive_universe_ticker(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as syn::ItemStruct);

    match universe::derive_ticker_inner(item) {
        Ok(output) => output.into(),
        Err((span, message)) => {
            quote_spanned!(span => compile_error!(#message);).into()
        },
    }
}


#[proc_macro_derive(WorldTicker)]
pub fn derive_world_ticker(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as syn::ItemStruct);

    match world::derive_ticker_inner(item) {
        Ok(output) => output.into(),
        Err((span, message)) => {
            quote_spanned!(span => compile_error!(#message);).into()
        },
    }
}


fn type_contains_ident(ty: Type, ident: &str) -> bool {
    match ty {
        Type::Array(array) => {
            return type_contains_ident(*array.elem, ident);
        },
        Type::Group(group) => {
            return type_contains_ident(*group.elem, ident);
        },
        Type::Paren(paren) => {
            return type_contains_ident(*paren.elem, ident);
        },
        Type::Path(path) => {
            let (last, _)  = get_last_ident(&path);
            last == ident
        },
        Type::Tuple(tuple) => {
            for elem in tuple.elems {
                if type_contains_ident(elem, ident.clone()) {
                    return true;
                }
            }
            return false;
        },
        _ => false,
    }
}

fn generic_arg_contains_ident(generic_arg: syn::GenericArgument, ident: &str) -> bool {
    match generic_arg {
        syn::GenericArgument::Type(ty) => return type_contains_ident(ty, ident),
        _ => false
    }
}

fn get_last_ident(path: &syn::TypePath) -> (proc_macro2::Ident, syn::PathArguments) {
    let last = path.path.segments.last().unwrap();
    (last.ident.clone(), last.arguments.clone())
}
