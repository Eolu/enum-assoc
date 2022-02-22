#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))]

use syn::{Error, Result, Variant, ItemFn, FnArg};
use proc_macro::TokenStream;
use quote::quote;

const FUNC_ATTR: &'static str = "func";
const ASSOC_ATTR: &'static str = "assoc";

#[proc_macro_derive(Assoc, attributes(func, assoc))]
pub fn derive_assoc(input: TokenStream) -> TokenStream 
{
    impl_macro(&syn::parse(input).expect("Failed to parse macro input"))
        // .map(|t| {println!("{}", quote!(#t)); t})
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn impl_macro(ast: &syn::DeriveInput) -> Result<proc_macro2::TokenStream>
{
    let name = &ast.ident;
    let fns = ast.attrs
        .iter()
        .filter(|attr| attr.path.is_ident(FUNC_ATTR))
        .map(|attr| parse_sig(&attr.tokens))
        .collect::<Result<Vec<ItemFn>>>()?;
    let variants: Vec<&Variant> = if let syn::Data::Enum(data) = &ast.data
    {
        data.variants.iter().collect()
    }
    else
    {
        panic!("#[derive(Assoc)] only applicable to enums")
    };
    let functions: Vec<proc_macro2::TokenStream> = fns.iter()
        .map(|func| build_function(&variants, &func))
        .collect::<Result<Vec<proc_macro2::TokenStream>>>()?;
    Ok(quote!
    {
        impl #name 
        {
            #(#functions)*
        }
    }.into())
}

fn build_function(variants: &[&Variant], func: &ItemFn) -> Result<proc_macro2::TokenStream>
{
    let vis = &func.vis;
    let sig = &func.sig;
    // has_self determines whether or not this a reverse assoc
    let has_self = match func.sig.inputs.first()
    {
        Some(FnArg::Receiver(_)) => true,
        Some(FnArg::Typed(pat_type)) => 
        {
            let pat = &pat_type.pat;
            quote!(#pat).to_string().trim() == "self"
        }
        None => false,
    };
    let is_option = if let syn::ReturnType::Type(_, ty) = &func.sig.output
    {
        let s = quote!(#ty).to_string();
        let trimmed = s.trim();
        trimmed.starts_with("Option") && trimmed.len() > 6 && trimmed[6..].trim().starts_with("<")
    }
    else
    {
        false
    };
    let mut arms = variants.iter()
        .map(|variant| build_variant_arm(variant, func, is_option, has_self))
        .collect::<Result<Vec<(proc_macro2::TokenStream, Wildcard)>>>()?;
    if is_option && !arms.iter().any(|(_, wildcard)| matches!(wildcard, Wildcard::True))
    { 
        arms.push((quote!(_ => None,), Wildcard::True))
    }
    // make sure wildcards are last
    if has_self == false
    {
        arms.sort_by(|(_, wildcard1), (_, wildcard2)| wildcard1.cmp(wildcard2));
    }
    let arms = arms.into_iter().map(|(toks, _)| toks);
    let match_on = if has_self
    {
        quote!(self)
    }
    else if func.sig.inputs.is_empty()
    {
        return Err(syn::Error::new_spanned(&func.sig, "Missing parameter"));
    }
    else
    {
        let mut result = quote!();
        for input in &func.sig.inputs
        {
            match input
            {
                FnArg::Receiver(_) => 
                {
                    result = quote!(self);
                    break;
                },
                FnArg::Typed(pat_type) => 
                {
                    let pat = &pat_type.pat;
                    result = if result.is_empty()
                    {
                        quote!(#pat)
                    }
                    else
                    {
                        quote!(#result, #pat)
                    };
                },
            }
        }
        if func.sig.inputs.len() > 1
        {
            result = quote!((#result));
        }
        result
    };
    Ok(quote!
    {
        #vis #sig
        {
            match #match_on
            {
                #(#arms)*
            }
        }
    })
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Wildcard
{
    False = 0,
    None = 1,
    True = 2
}

fn build_variant_arm(variant: &Variant, func: &ItemFn, is_option: bool, has_self: bool) -> Result<(proc_macro2::TokenStream, Wildcard)>
{
    let var_ident = &variant.ident;
    // TODO: These have to be handled quite differently for reverse assoc
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
    // Parse associations
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
            let r: (syn::Ident, proc_macro2::TokenStream) = 
            (
                match s[..first_eq].trim().parse()
                {
                    Ok(fn_name) => syn::parse2::<syn::Ident>(fn_name)?,
                    Err(_) => return Err(Error::new_spanned(attr, "Invalid `assoc` attribute, failed to parse left side"))
                },
                s[first_eq+1..].trim().parse::<proc_macro2::TokenStream>()?
            );
            Ok(r)
            
        })
        .filter(|result| 
        {
            match result
            {
                Ok((inner_name, _)) => *inner_name == func.sig.ident,
                Err(_) => true
            }
        });
    if has_self
    {
        let assocs = assocs.map(|result| 
            {
                match result
                {
                    Ok((_, val)) => syn::parse2::<syn::Expr>(val),
                    Err(e) => Err(e)
                }
            })
            .collect::<Result<Vec<syn::Expr>>>()?;
        match assocs.len()
        {
            0 if is_option => Ok(quote!{ Self::#var_ident #fields => None, }),
            0 => Err(Error::new_spanned(variant, format!("Missing `assoc` attribute for {}", func.sig.ident))),
            1 => 
            {
                let val = &assocs[0];
                if is_option
                {
                    if quote!(#val).to_string().trim() == "None"
                    {
                        Ok(quote!{ Self::#var_ident #fields => #val, })
                    }
                    else
                    {
                        Ok(quote!{ Self::#var_ident #fields => Some(#val), })
                    }
                }
                else
                {
                    Ok(quote!{ Self::#var_ident #fields => #val, })
                }
            }
            _ => Err(Error::new_spanned(variant, format!("Too many `assoc` attributes for {}", func.sig.ident)))
        }.map(|toks| (toks, Wildcard::None))
    }
    else
    {
        let assocs = assocs.map(|result| 
            {
                match result
                {
                    Ok((_, val)) => syn::parse2::<syn::Pat>(val),
                    Err(e) => Err(e)
                }
            })
            .collect::<Result<Vec<syn::Pat>>>()?;
        let mut result = quote!();
        let mut pat_catch_all = false;
        for pat in assocs.iter()
        {
            if !fields.is_empty()
            {
                return Err(Error::new_spanned(variant, "Reverse associations not allowed for tuple or struct-like variants"))
            }
            let arm = if is_option
            {
                quote!(#pat => Some(Self::#var_ident),)
            }
            else
            {
                quote!(#pat => Self::#var_ident,)
            };
            result = if matches!(pat, syn::Pat::Wild(_))
            {
                if pat_catch_all
                {
                    return Err(syn::Error::new_spanned(pat, "Only 1 wildcard allowed per reverse association"))
                }
                pat_catch_all = true;
                quote!(#result #arm)
            }
            else
            {
                quote!(#arm #result)
            };
        }
        Ok((result, if pat_catch_all {Wildcard::True} else {Wildcard::False}))
    }
}

/// Parse a function signature from an atribute
fn parse_sig(tokens: &proc_macro2::TokenStream) -> Result<ItemFn>
{
    let s = tokens.to_string();
    if s.len() > 2
    {
        let s = format!("{}{{}}", &s[1..s.len()-1]);
        syn::parse_str::<ItemFn>(&s)
    }
    else
    {
        Err(syn::Error::new_spanned(tokens, "Missing function signature"))
    }
}