#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))]

use syn::{Error, Result, Variant};
use proc_macro::TokenStream;
use quote::quote;

const FUNC_ATTR: &'static str = "func";
const ASSOC_ATTR: &'static str = "assoc";

#[proc_macro_derive(Assoc, attributes(func, assoc))]
pub fn derive_assoc(input: TokenStream) -> TokenStream 
{
    impl_macro(&syn::parse(input).expect("Failed to parse macro input"))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn impl_macro(ast: &syn::DeriveInput) -> Result<proc_macro2::TokenStream>
{
    let name = &ast.ident;
    let fn_attrs = ast.attrs
        .iter()
        .filter(|attr| attr.path.is_ident(FUNC_ATTR))
        .map(|attr|
        {
            let s = attr.tokens.to_string();
            match s[1..s.len()-1].parse::<proc_macro2::TokenStream>()
            {
                Ok(result) => Ok(result),
                _ => Err(Error::new_spanned(attr, "Failed to parse attribute"))
            }
        })
        .collect::<Result<Vec<proc_macro2::TokenStream>>>()?;
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
    let variants: Vec<&Variant> = if let syn::Data::Enum(data) = &ast.data
    {
        data.variants.iter().collect()
    }
    else
    {
        panic!("#[derive(Assoc)] only applicable to enums")
    };
    let functions: Vec<proc_macro2::TokenStream> = fn_attrs.iter()
        .zip(fn_names.iter())
        .zip(fn_options.into_iter())
        .map(|((attr, fn_name), is_option)| build_variant_arms(&variants, attr, fn_name, is_option))
        .collect::<Result<Vec<proc_macro2::TokenStream>>>()?;
    Ok(quote!
    {
        impl #name 
        {
            #(#functions)*
        }
    }.into())
}

fn build_variant_arms(variants: &Vec<&Variant>, attr: &proc_macro2::TokenStream, fn_name: &str, is_option: bool)
     -> Result<proc_macro2::TokenStream>
{
    let arms: Vec<proc_macro2::TokenStream> = variants.iter()
        .map(|variant| build_variant_arm(variant, fn_name, is_option))
        .collect::<Result<Vec<proc_macro2::TokenStream>>>()?;
    Ok(quote!
    {
        #attr
        {
            match self
            {
                #(#arms),*
            }
        }
    })
}

fn build_variant_arm(variant: &Variant, fn_name: &str, is_option: bool) -> Result<proc_macro2::TokenStream>
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
        .map(|attr| 
        {
            let s = attr.tokens.to_string();
            let s = &s[1..s.len()-1];
            let first_eq = match s.find("=")
            {
                Some(result) => result,
                None => return Err(Error::new_spanned(attr, "Invalid `assoc` attribute, missing `=`"))
            };
            let r: (proc_macro2::TokenStream, proc_macro2::TokenStream) = 
            (
                match s[..first_eq].trim().parse()
                {
                    Ok(fn_name) => fn_name,
                    Err(_) => return Err(Error::new_spanned(attr, "Invalid `assoc` attribute, failed to parse left side"))
                },
                match s[first_eq+1..].trim().parse()
                {
                    Ok(val) => val,
                    Err(_) => return Err(Error::new_spanned(attr, "Invalid `assoc` attribute, failed to parse right side"))
                }
            );
            Ok(r)
            
        })
        .filter(|result| 
        {
            match result
            {
                Ok((inner_name, _)) => inner_name.to_string() == fn_name,
                Err(_) => true
            }
        })
        .map(|result| 
        {
            match result
            {
                Ok((_, val)) => Ok(val),
                Err(e) => Err(e)
            }
        })
        .collect::<Result<Vec<proc_macro2::TokenStream>>>()?;
    match assocs.len()
    {
        0 if is_option => Ok(quote!{ Self::#var_ident #fields => None }),
        0 => Err(Error::new_spanned(variant, format!("Missing `assoc` attribute for {}", fn_name))),
        1 => 
        {
            let val = &assocs[0];
            if is_option
            {
                if 
                {
                    let s = val.to_string();
                    let trimmed = s.trim();
                    trimmed.starts_with("Some") && trimmed[4..].trim().starts_with("(")
                }
                {
                    Ok(quote!{ Self::#var_ident #fields => #val })
                }
                else
                {
                    Ok(quote!{ Self::#var_ident #fields => Some(#val) })
                }
            }
            else
            {
                Ok(quote!{ Self::#var_ident #fields => #val })
            }
        }
        _ => Err(Error::new_spanned(variant, format!("Too many `assoc` attributes for {}", fn_name)))
    }
}