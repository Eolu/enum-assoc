#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))]

use proc_macro::TokenStream;
use quote::quote;

const FUNC_ATTR: &'static str = "func";
const ASSOC_ATTR: &'static str = "assoc";

#[proc_macro_derive(Assoc, attributes(func, assoc))]
pub fn derive_assoc(input: TokenStream) -> TokenStream 
{
    impl_macro(&syn::parse(input).expect("Faield ot parse input"))
}

fn impl_macro(ast: &syn::DeriveInput) -> TokenStream
{
    let name = &ast.ident;
    let fn_attrs = ast.attrs
        .iter()
        .filter(|attr| attr.path.is_ident(FUNC_ATTR))
        .map(|attr|
        {
            let s = attr.tokens.to_string();
            s[1..s.len()-1].parse().expect("Failed to parse attribute")
        })
        .collect::<Vec<proc_macro2::TokenStream>>();
    let fn_names = fn_attrs.iter()
        .filter_map
        (
            |f| f.to_string()
                .split("fn ")
                .skip(1)
                .next()
                .map(|s| s.split("(").map(|s| s.trim()).next())
                .map(|s| s.map(|s| s.to_string()))
        )
        .flatten()
        .collect::<Vec<String>>();
    let fn_options = fn_attrs.iter()
        .filter_map
        (
            |f| f.to_string()
                .split("->")
                .last()
                .map(|s| 
                {
                    let trimmed = s.trim();
                    trimmed.starts_with("Option") && trimmed[6..].trim().starts_with("<")
                })
        )
        .collect::<Vec<bool>>();
    let variants = if let syn::Data::Enum(data) = &ast.data
    {
        data.variants.iter().collect::<Vec<_>>()
    }
    else
    {
        panic!("#[derive(Assoc)] only applicable to enums")
    };
    let functions = fn_attrs.iter().zip(fn_names.iter()).zip(fn_options.into_iter()).map
    ( |((attr, name), is_option)|
    {
        let arms = variants.iter().map(move |variant| 
        {
            let var_ident = &variant.ident;
            let fields = match &variant.fields
            {
                syn::Fields::Named(fields) => 
                {
                    let named = fields.named.iter().map(|f| 
                    {
                        let ident = &f.ident;
                        quote!(#ident: _)
                    }).collect::<Vec::<proc_macro2::TokenStream>>();
                    quote!({#(#named),*})
                },
                syn::Fields::Unnamed(fields) => 
                {
                    let unnamed = fields.unnamed.iter().map(|_| quote!(_)).collect::<Vec::<proc_macro2::TokenStream>>();
                    quote!((#(#unnamed),*))
                },
                _ => quote!()
            };
            let assocs = variant
                .attrs
                .iter()
                .filter(|assoc_attr| assoc_attr.path.is_ident(ASSOC_ATTR))
                .map(|attr| extract_val(&attr.tokens))
                .filter(|(inner_name, _)| inner_name.to_string() == name.as_str())
                .map(|(_, val)| val)
                .collect::<Vec<proc_macro2::TokenStream>>();
            match assocs.len()
            {
                0 if is_option => quote!{ Self::#var_ident #fields => None },
                0 => panic!("Missing `assoc` attribute for `{}`", name),
                1 => 
                {
                    let val = &assocs[0];
                    if is_option
                    {
                        if is_some(val)
                        {
                            quote!{ Self::#var_ident #fields => #val }
                        }
                        else
                        {
                            quote!{ Self::#var_ident #fields => Some(#val) }
                        }
                    }
                    else
                    {
                        quote!{ Self::#var_ident #fields => #val }
                    }
                }
                _ => panic!("Too many `assoc` attributes for `{}`", name)
            }
        });
        quote!
        {
            #attr
            {
                match self
                {
                    #(#arms),*
                }
            }
        }
    }
    ).collect::<Vec<_>>();
    quote!
    {
        impl #name 
        {
            #(#functions)*
        }
    }.into()
}

fn extract_val(t: &proc_macro2::TokenStream) -> (proc_macro2::TokenStream, proc_macro2::TokenStream)
{
    let s = t.to_string();
    let s = &s[1..s.len()-1];
    let first_eq = s.find("=").expect("Invalid `assoc` attribute");
    (
        // name
        s[..first_eq].trim().parse().expect("Invalid `assoc` attribute"), 
        // val
        s[first_eq+1..].trim().parse().expect("Invalid `assoc` attribute")
    )
}

fn is_some(t: &proc_macro2::TokenStream) -> bool
{
    let s = t.to_string();
    let trimmed = s.trim();
    trimmed.starts_with("Some") && trimmed[4..].trim().starts_with("(")
}