use proc_macro2::{TokenStream as TokenStream2, Span};
use quote::quote;
use syn::{ItemStruct, PathArguments::AngleBracketed};

pub(crate) fn derive_ticker_inner(item: ItemStruct) -> Result<TokenStream2, (Span, String)> {
    let ident = item.ident;

    let mut method_update_universe_ptr = TokenStream2::new();
    let mut method_tick = TokenStream2::new();
    let mut method_update_children_ptr = TokenStream2::new();

    for field in item.fields {
        let field_ident = field.ident.clone().expect("tuple structs are unsupported");
        parse_field(field.ty.clone(), field_ident, &mut method_update_universe_ptr, &mut method_tick, &mut method_update_children_ptr)?;
    }

    Ok(quote! {
        unsafe impl graphite_server::ticker::WorldTicker<Self> for #ident {
            fn update_universe_ptr(&mut self, universe: *mut Universe<<Self as WorldService>::UniverseServiceType>) {
                #method_update_universe_ptr
            }

            fn update_children_ptr(&mut self, universe: *mut World<Self>) {
                #method_update_children_ptr
            }
        
            fn tick(&mut self, tick_phase: TickPhase) {
                #method_tick
            }
        }
    }.into())
}

fn parse_field(ty: syn::Type, field_ident: syn::Ident, method_update_universe_ptr: &mut TokenStream2, method_tick: &mut TokenStream2, method_update_children_ptr: &mut TokenStream2) -> Result<(), (Span, String)> {
    match ty.clone() {
        syn::Type::Array(_) => {
            if crate::type_contains_ident(ty.clone(), "World") {
                return Err((field_ident.span(), "Array containing World<W> is unsupported".into()))
            } else if crate::type_contains_ident(ty, "Player") {
                return Err((field_ident.span(), "Array containing Player<W> is unsupported".into()))
            }
        },
        syn::Type::Group(group) => {
            parse_field(*group.elem, field_ident, method_update_universe_ptr, method_tick, method_update_children_ptr)?;
        },
        syn::Type::Paren(paren) => {
            parse_field(*paren.elem, field_ident, method_update_universe_ptr, method_tick, method_update_children_ptr)?;
        },
        syn::Type::Path(path) => {
            let (last, args) = crate::get_last_ident(&path);

            if last == "PlayerVec" {
                *method_tick = quote! {
                    #method_tick
                    self.#field_ident.tick(tick_phase);
                };
                *method_update_children_ptr = quote! {
                    #method_update_children_ptr
                    self.#field_ident.update_world_ptr(universe);
                };
            } else if last == "World" {
                *method_update_universe_ptr = quote! {
                    #method_update_universe_ptr
                    self.#field_ident.update_universe_ptr(world);
                };
                *method_tick = quote! {
                    #method_tick
                    if tick_phase.is_update() {
                        self.#field_ident.tick();
                    }
                };
                *method_update_children_ptr = quote! {
                    #method_update_children_ptr
                    self.#field_ident.update_parent_world_ptr(world);
                    self.#field_ident.initialize();
                };
            // todo: add WorldVec
            } else if last == "WorldMap" {
                *method_update_universe_ptr = quote! {
                    #method_update_universe_ptr
                    self.#field_ident.update_universe_ptr(world);
                };
                *method_tick = quote! {
                    #method_tick
                    if tick_phase.is_update() {
                        self.#field_ident.tick();
                    }
                };
                *method_update_children_ptr = quote! {
                    #method_update_children_ptr
                    self.#field_ident.update_parent_world_ptr(universe);
                };
            } else if last == "Vec" {
                match args {
                    AngleBracketed(bracketed) => {
                        if bracketed.args.len() == 1 {
                            let generic_arg = &bracketed.args[0];
                            if crate::generic_arg_contains_ident(generic_arg.clone(), "World") {
                                // todo: tell them to use WorldVec instead
                                return Err((last.span(), "Vec containing World<W> is unsupported".into()))
                            } else if crate::generic_arg_contains_ident(generic_arg.clone(), "Player") {
                                return Err((last.span(), "Vec containing Player<P> is unsupported, use PlayerVec<W> instead".into()))
                            }
                        }
                    },
                    _ => ()
                }
            } else if last == "HashMap" || last == "BTreeMap" || last == "StickyMap" {
                match args {
                    AngleBracketed(bracketed) => {
                        if bracketed.args.len() == 2 {
                            let generic_arg = &bracketed.args[1];
                            if crate::generic_arg_contains_ident(generic_arg.clone(), "World") {
                                return Err((last.span(), "Map containing World<W> is unsupported, use WorldMap<W> instead".into()))
                                // todo: tell them to use PlayerMap instead
                            } else if crate::generic_arg_contains_ident(generic_arg.clone(), "Player") {
                                return Err((last.span(), "Map containing Player<P> is unsupported".into()))
                            }
                        }
                    },
                    _ => ()
                }
            }
        },
        syn::Type::Tuple(_) => {
            if crate::type_contains_ident(ty.clone(), "World") {
                return Err((field_ident.span(), "Tuple containing World<W> is unsupported".into()))
            } else if crate::type_contains_ident(ty, "Player") {
                return Err((field_ident.span(), "Tuple containing Player<W> is unsupported".into()))
            }
        },
        _ => (),
    }
    return Ok(())
}