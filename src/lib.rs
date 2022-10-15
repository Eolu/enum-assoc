#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))]

use syn::{Error, Result, Variant, ItemFn, FnArg, Attribute, spanned::Spanned};
use proc_macro::TokenStream;
use quote::quote;

const FUNC_ATTR: &'static str = "func";
const ASSOC_ATTR: &'static str = "assoc";

#[proc_macro_derive(Assoc, attributes(func, assoc))]
pub fn derive_assoc(input: TokenStream) -> TokenStream 
{
    impl_macro(&syn::parse(input).expect("Failed to parse macro input"))
        //.map(|t| {println!("{}", quote!(#t)); t})
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn impl_macro(ast: &syn::DeriveInput) -> Result<proc_macro2::TokenStream>
{
    let name = &ast.ident;
    let generics = &ast.generics;
    let generic_params = &generics.params;
    let fns = ast.attrs
        .iter()
        .filter(|attr| attr.path.is_ident(FUNC_ATTR))
        .map(|attr| DeriveFunc::parse_sig(&attr.tokens))
        .collect::<Result<Vec<DeriveFunc>>>()?;
    let variants: Vec<&Variant> = if let syn::Data::Enum(data) = &ast.data
    {
        data.variants.iter().collect()
    }
    else
    {
        panic!("#[derive(Assoc)] only applicable to enums")
    };
    let functions: Vec<proc_macro2::TokenStream> = fns.into_iter()
        .map(|func| build_function(&variants, func))
        .collect::<Result<Vec<proc_macro2::TokenStream>>>()?;
    Ok(quote!
    {
        impl <#generic_params> #name #generics
        {
            #(#functions)*
        }
    }.into())
}

fn build_function(variants: &[&Variant], func: DeriveFunc) -> Result<proc_macro2::TokenStream>
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
        .map(|variant| build_variant_arm(variant, &func.sig.ident, is_option, has_self, &func.def))
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
        return Err(syn::Error::new(func.span, "Missing parameter"));
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

fn build_variant_arm(variant: &Variant, func: &syn::Ident, is_option: bool, has_self: bool, def: &Option<proc_macro2::TokenStream>) -> Result<(proc_macro2::TokenStream, Wildcard)>
{
    // Partially parse associations
    let assocs = Association::get_variant_assocs(variant, !has_self)
        .filter(|result| 
        {
            match result
            {
                Ok(assoc) => assoc.func == *func,
                Err(_) => true
            }
        });
    if has_self
    {
        build_fwd_assoc(assocs, variant, is_option, func, def)
    }
    else
    {
        build_rev_assoc(assocs, variant, is_option)
    }
}

fn build_fwd_assoc(assocs: impl Iterator<Item = Result<Association>>, variant: &Variant, is_option: bool, func_ident: &syn::Ident, def: &Option<proc_macro2::TokenStream>) 
    -> Result<(proc_macro2::TokenStream, Wildcard)>
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
    let assocs = assocs
        .map(|assoc| assoc.map(|assoc| assoc.assoc.unwrap_expr()))
        .collect::<Result<Vec<syn::Expr>>>()?;
    match assocs.len()
    {
        0 => if let Some(tokens) = def 
            {
                Ok(quote!{ Self::#var_ident #fields => #tokens, })
            } 
            else if is_option
            {
                Ok(quote!{ Self::#var_ident #fields => None, })
            }
            else
            {
                Err(Error::new_spanned(variant, format!("Missing `assoc` attribute for {}", func_ident)))
            },
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
        _ => Err(Error::new_spanned(variant, format!("Too many `assoc` attributes for {}", func_ident)))
    }.map(|toks| (toks, Wildcard::None))
}

fn build_rev_assoc(assocs: impl Iterator<Item = Result<Association>>, variant: &Variant, is_option: bool) 
    -> Result<(proc_macro2::TokenStream, Wildcard)>
{
    let var_ident = &variant.ident;
    let assocs = assocs
        .map(|assoc| assoc.map(|assoc| assoc.assoc.unwrap_pat()))
        .collect::<Result<Vec<syn::Pat>>>()?;
    let mut concrete_pats: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut wildcard_pat: Option<proc_macro2::TokenStream> = None;
    let mut wildcard_status = Wildcard::False;
    for pat in assocs.iter()
    {
        if !matches!(variant.fields, syn::Fields::Unit)
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
        if matches!(pat, syn::Pat::Wild(_))
        {
            if wildcard_pat.is_some()
            {
                return Err(syn::Error::new_spanned(pat, "Only 1 wildcard allowed per reverse association"))
            }
            wildcard_status = Wildcard::True;
            wildcard_pat = Some(arm);
        }
        else
        {
            concrete_pats.push(arm);
        }
    }
    if let Some(wildcard_pat) = wildcard_pat
    {
        concrete_pats.push(wildcard_pat)
    }
    Ok((quote!(#(#concrete_pats) *), wildcard_status))
}

/// A container for a function parsed within a `func` attribute. Note that the 
/// span of the `func` atribute is included because the syn nodes were 
/// manipulated as a string and have lost therr own span information.
struct DeriveFunc
{
    vis: syn::Visibility,
    sig: syn::Signature,
    span: proc_macro2::Span,
    def: Option<proc_macro2::TokenStream>
}

/// An association. Contains a function ident as well as the actual tokens of
/// the VALUE (not the variant) of the association. 
struct Association
{
    func: syn::Ident,
    assoc: AssociationType
}

/// An expression for a forward association, a pattern for a reverse 
/// association.
enum AssociationType
{
    Expr(syn::Expr),
    Pat(syn::Pat)
}

/// For reverse associations, this enum keeps track of wldcard patterns. For 
/// forward associations, the value is always set to "None". This is also used
/// to sort reverse associations appropriately. If more complex sorting is to
/// be implemented, updating this enum would be the best way to start.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Wildcard
{
    False = 0,
    None = 1,
    True = 2
}

impl DeriveFunc
{
    /// Parse a function signature from an attribute
    fn parse_sig(tokens: &proc_macro2::TokenStream) -> Result<Self>
    {
        let mut s = tokens.to_string();
        if s.len() > 2
        {
            s = format!("{}", &s[1..s.len()-1]);
            let has_default = &s[s.len()-1..s.len()] == "}";
            
            if !has_default {
                s = format!("{}{{}}", s);
            }
            let fn_item = syn::parse_str::<ItemFn>(&s)?;
            
            let def = 
                if has_default
                    {
                        Some(proc_macro2::TokenStream::from(quote::ToTokens::into_token_stream(fn_item.block)))
                    }
                    else
                    {
                        None
                    };
            Ok(DeriveFunc{ vis: fn_item.vis, sig: fn_item.sig,span: tokens.span(), def })
        }
        else
        {
            Err(syn::Error::new_spanned(tokens, "Missing function signature"))
        }
    }
}

impl Association
{
    fn new(attr: &Attribute, is_reverse: bool) -> Result<Self>
    {
        let s = attr.tokens.to_string();
        let s = &s[1..s.len()-1];
        let first_eq = match s.find("=")
        {
            Some(result) => result,
            None => return Err(Error::new_spanned(attr, "Invalid `assoc` attribute, missing `=`"))
        };
        Ok(Association
        {
            func: match s[..first_eq].trim().parse()
            {
                Ok(fn_name) => syn::parse2::<syn::Ident>(fn_name)?,
                Err(_) => return Err(Error::new_spanned(attr, "Invalid `assoc` attribute, failed to parse left side"))
            },
            assoc: AssociationType::new(s[first_eq+1..].trim().parse::<proc_macro2::TokenStream>()?, is_reverse)?
        })
    }

    fn get_variant_assocs(variant: &Variant, is_reverse: bool) -> impl Iterator<Item = Result<Self>> + '_
    {
        variant
            .attrs
            .iter()
            .filter(|assoc_attr| assoc_attr.path.is_ident(ASSOC_ATTR))
            .map(move |attr| Association::new(attr, is_reverse))
    }
}

impl AssociationType
{
    fn new(tokens: proc_macro2::TokenStream, is_reverse: bool) -> Result<Self>
    {
        if is_reverse
        {
            syn::parse2::<syn::Pat>(tokens).map(AssociationType::Pat)
        }
        else
        {
            syn::parse2::<syn::Expr>(tokens).map(AssociationType::Expr)
        }
    }

    /// Appllicable to forward associations only
    fn unwrap_expr(self) -> syn::Expr
    {
        if let Self::Expr(expr) = self
        {
            expr
        }
        else
        {
            // This should be unreachable. Seeing this means a forward
            // association was parsed as if it were a reverse association.
            panic!("Attempted to unwrap pattern as expression")
        }
    }

    /// Appllicable to reverse associations only
    fn unwrap_pat(self) -> syn::Pat
    {
        if let Self::Pat(pat) = self
        {
            pat
        }
        else
        {
            // This should be unreachable. Seeing this means a reverse 
            // association was parsed as if it were a forward association.
            panic!("Attempted to unwrap expression as pattern")
        }
    }
}
