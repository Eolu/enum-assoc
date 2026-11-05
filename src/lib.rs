#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))]

use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use syn::{
    Error, FnArg, Result, Token, Type, Variant, braced, parenthesized, parse::Parser,
    punctuated::Punctuated, spanned::Spanned,
};

const FUNC_ATTR: &str = "func";
const ASSOC_ATTR: &str = "assoc";

#[proc_macro_derive(Assoc, attributes(func, assoc))]
pub fn derive_assoc(input: TokenStream) -> TokenStream {
    impl_macro(&syn::parse(input).expect("Failed to parse macro input"))
        //.map(|t| {println!("{}", quote!(#t)); t})
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn impl_macro(ast: &syn::DeriveInput) -> Result<proc_macro2::TokenStream> {
    let name = &ast.ident;
    let generics = &ast.generics;
    let generic_params = &generics.params;
    let where_clause = &generics.where_clause;
    let fns = ast
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident(FUNC_ATTR))
        .map(|attr| syn::parse2::<DeriveFuncs>(attr.meta.to_token_stream()))
        .collect::<Result<Vec<DeriveFuncs>>>()?;
    let variants: Vec<&Variant> = if let syn::Data::Enum(data) = &ast.data {
        data.variants.iter().collect()
    } else {
        panic!("#[derive(Assoc)] only applicable to enums")
    };
    let mut impls: Vec<proc_macro2::TokenStream> = Vec::new();
    for DeriveFuncs(items) in fns {
        let mut associated_funcs = Vec::new();
        for item in &items {
            match item {
                DeriveItem::Func(func) => associated_funcs.push(func.assoc_name.clone()),
                DeriveItem::Impl(imp) => {
                    associated_funcs.extend(imp.funcs.iter().map(|f| f.assoc_name.clone()))
                }
            }
        }
        let mut functions = Vec::new();
        for item in items {
            match item {
                DeriveItem::Func(func) => functions.push(build_function(
                    &variants,
                    &func,
                    &associated_funcs,
                    name,
                    false,
                )?),
                DeriveItem::Impl(imp) => {
                    // A trait implementation for a type other than the enum cannot
                    // return enum variants, so all of its methods are forward
                    // associations even when they have no `self` parameter.
                    let trait_impl_for_other =
                        imp.trait_.is_some() && !type_is_enum(&imp.self_ty, name);
                    let mut methods = Vec::new();
                    for func in &imp.funcs {
                        if imp.trait_.is_some() && !matches!(func.vis, syn::Visibility::Inherited) {
                            return Err(Error::new_spanned(
                                &func.vis,
                                "Trait implementation methods cannot have a visibility modifier",
                            ));
                        }
                        methods.push(build_function(
                            &variants,
                            func,
                            &associated_funcs,
                            name,
                            trait_impl_for_other,
                        )?);
                    }
                    impls.push(build_impl(imp, &methods));
                }
            }
        }
        if !functions.is_empty() {
            impls.push(quote! {
                #[allow(clippy::used_underscore_binding)]
                impl <#generic_params> #name #generics #where_clause
                {
                    #(#functions)*
                }
            });
        }
    }
    Ok(quote! { #(#impls)* })
}

fn build_impl(imp: DeriveImpl, methods: &[proc_macro2::TokenStream]) -> proc_macro2::TokenStream {
    let unsafety = &imp.unsafety;
    let generics = &imp.generics;
    let where_clause = &generics.where_clause;
    let self_ty = &imp.self_ty;
    let header = if let Some((bang, trait_path, for_token)) = &imp.trait_ {
        quote!(impl #generics #bang #trait_path #for_token #self_ty #where_clause)
    } else {
        quote!(impl #generics #self_ty #where_clause)
    };
    quote! {
        #[allow(clippy::used_underscore_binding)]
        #unsafety #header
        {
            #(#methods)*
        }
    }
}

fn type_is_enum(ty: &Type, name: &syn::Ident) -> bool {
    match ty {
        Type::Path(type_path) if type_path.qself.is_none() => type_path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == *name),
        Type::Group(group) => type_is_enum(&group.elem, name),
        Type::Paren(paren) => type_is_enum(&paren.elem, name),
        _ => false,
    }
}

fn build_function(
    variants: &[&Variant],
    func: &DeriveFunc,
    associated_funcs: &[syn::Ident],
    enum_name: &syn::Ident,
    trait_impl_for_other: bool,
) -> Result<proc_macro2::TokenStream> {
    let vis = &func.vis;
    let sig = &func.sig;
    // has_self determines whether or not this a reverse assoc, except for trait
    // implementations on other types, which can only use forward associations.
    let has_self = match func.sig.inputs.first() {
        Some(FnArg::Receiver(_)) => true,
        Some(FnArg::Typed(pat_type)) => {
            let pat = &pat_type.pat;
            quote!(#pat).to_string().trim() == "self"
        }
        None => false,
    };
    let is_reverse = !has_self && !trait_impl_for_other;
    // `Self` is the enum in inherent impls and in trait impls for the enum
    // itself, but it is a different type in trait impls on other types.
    let enum_path = if trait_impl_for_other {
        quote!(#enum_name)
    } else {
        quote!(Self)
    };
    let is_option = if let syn::ReturnType::Type(_, ty) = &func.sig.output {
        let s = quote!(#ty).to_string();
        let trimmed = s.trim();
        trimmed.starts_with("Option") && trimmed.len() > 6 && trimmed[6..].trim().starts_with("<")
    } else {
        false
    };
    let mut arms = variants
        .iter()
        .map(|variant| {
            build_variant_arm(
                variant,
                &func.assoc_name,
                associated_funcs.iter().cloned(),
                is_option,
                is_reverse,
                &func.def,
                &enum_path,
            )
        })
        .collect::<Result<Vec<(proc_macro2::TokenStream, Wildcard)>>>()?;
    if is_option
        && !arms
            .iter()
            .any(|(_, wildcard)| matches!(wildcard, Wildcard::True))
    {
        arms.push((quote!(_ => None,), Wildcard::True))
    }
    // make sure wildcards are last
    if is_reverse {
        arms.sort_by(|(_, wildcard1), (_, wildcard2)| wildcard1.cmp(wildcard2));
    }
    let arms = arms.into_iter().map(|(toks, _)| toks);
    let match_on = if is_reverse {
        let mut result = quote!();
        for input in &func.sig.inputs {
            match input {
                FnArg::Receiver(_) => {
                    result = quote!(self);
                    break;
                }
                FnArg::Typed(pat_type) => {
                    let pat = &pat_type.pat;
                    result = if result.is_empty() {
                        quote!(#pat)
                    } else {
                        quote!(#result, #pat)
                    };
                }
            }
        }
        if func.sig.inputs.is_empty() {
            return Err(syn::Error::new(func.span, "Missing parameter"));
        }
        if func.sig.inputs.len() > 1 {
            result = quote!((#result));
        }
        result
    } else if has_self {
        quote!(self)
    } else {
        match func.sig.inputs.first() {
            Some(FnArg::Typed(pat_type)) => {
                let pat = &pat_type.pat;
                quote!(#pat)
            }
            _ => return Err(syn::Error::new(func.span, "Missing parameter")),
        }
    };
    Ok(quote! {
        #vis #sig
        {
            match #match_on
            {
                #(#arms)*
            }
        }
    })
}

fn build_variant_arm(
    variant: &Variant,
    func: &syn::Ident,
    mut assoc_funcs: impl Iterator<Item = syn::Ident>,
    is_option: bool,
    is_reverse: bool,
    def: &Option<proc_macro2::TokenStream>,
    enum_path: &proc_macro2::TokenStream,
) -> Result<(proc_macro2::TokenStream, Wildcard)> {
    // Partially parse associations
    let assocs = Association::get_variant_assocs(variant, is_reverse).filter(|assoc| {
        assoc.func == *func || assoc_funcs.any(|assoc_func| assoc_func == assoc.func)
    });
    if is_reverse {
        build_rev_assoc(assocs, variant, is_option, enum_path)
    } else {
        build_fwd_assoc(assocs, variant, is_option, func, def, enum_path)
    }
}

fn build_fwd_assoc(
    assocs: impl Iterator<Item = Association>,
    variant: &Variant,
    is_option: bool,
    func_ident: &syn::Ident,
    def: &Option<proc_macro2::TokenStream>,
    enum_path: &proc_macro2::TokenStream,
) -> Result<(proc_macro2::TokenStream, Wildcard)> {
    let var_ident = &variant.ident;
    let fields = match &variant.fields {
        syn::Fields::Named(fields) => {
            let named = fields
                .named
                .iter()
                .map(|f| {
                    let ident = &f.ident;
                    let val: &Option<proc_macro2::Ident> = &f.ident.as_ref().map(|s| {
                        proc_macro2::Ident::new(&("_".to_string() + &s.to_string()), f.span())
                    });
                    quote!(#ident: #val)
                })
                .collect::<Vec<proc_macro2::TokenStream>>();
            quote!({#(#named),*})
        }
        syn::Fields::Unnamed(fields) => {
            let unnamed = fields
                .unnamed
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let ident =
                        proc_macro2::Ident::new(&("_".to_string() + &i.to_string()), f.span());
                    quote!(#ident)
                })
                .collect::<Vec<proc_macro2::TokenStream>>();
            quote!((#(#unnamed),*))
        }
        _ => quote!(),
    };
    let assocs = assocs
        .filter_map(|assoc| {
            if let AssociationType::Forward(expr) = assoc.assoc {
                Some(Ok(expr))
            } else {
                None
            }
        })
        .collect::<Result<Vec<syn::Expr>>>()?;
    match assocs.len() {
        0 => {
            if let Some(tokens) = def {
                Ok(quote! { #enum_path::#var_ident #fields => #tokens, })
            } else if is_option {
                Ok(quote! { #enum_path::#var_ident #fields => None, })
            } else {
                Err(Error::new_spanned(
                    variant,
                    format!("Missing `assoc` attribute for {}", func_ident),
                ))
            }
        }
        1 => {
            let val = &assocs[0];
            if is_option {
                if quote!(#val).to_string().trim() == "None" {
                    Ok(quote! { #enum_path::#var_ident #fields => #val, })
                } else {
                    Ok(quote! { #enum_path::#var_ident #fields => Some(#val), })
                }
            } else {
                Ok(quote! { #enum_path::#var_ident #fields => #val, })
            }
        }
        _ => Err(Error::new_spanned(
            variant,
            format!("Too many `assoc` attributes for {}", func_ident),
        )),
    }
    .map(|toks| (toks, Wildcard::None))
}

fn build_rev_assoc(
    assocs: impl Iterator<Item = Association>,
    variant: &Variant,
    is_option: bool,
    enum_path: &proc_macro2::TokenStream,
) -> Result<(proc_macro2::TokenStream, Wildcard)> {
    let var_ident = &variant.ident;
    let assocs = assocs
        .filter_map(|assoc| {
            if let AssociationType::Reverse(pat) = assoc.assoc {
                Some(Ok(pat))
            } else {
                None
            }
        })
        .collect::<Result<Vec<syn::Pat>>>()?;
    let mut concrete_pats: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut wildcard_pat: Option<proc_macro2::TokenStream> = None;
    let mut wildcard_status = Wildcard::False;
    for pat in assocs.iter() {
        if !matches!(variant.fields, syn::Fields::Unit) {
            return Err(Error::new_spanned(
                variant,
                "Reverse associations not allowed for tuple or struct-like variants",
            ));
        }
        let arm = if is_option {
            quote!(#pat => Some(#enum_path::#var_ident),)
        } else {
            quote!(#pat => #enum_path::#var_ident,)
        };
        if matches!(pat, syn::Pat::Wild(_)) {
            if wildcard_pat.is_some() {
                return Err(syn::Error::new_spanned(
                    pat,
                    "Only 1 wildcard allowed per reverse association",
                ));
            }
            wildcard_status = Wildcard::True;
            wildcard_pat = Some(arm);
        } else {
            concrete_pats.push(arm);
        }
    }
    if let Some(wildcard_pat) = wildcard_pat {
        concrete_pats.push(wildcard_pat)
    }
    Ok((quote!(#(#concrete_pats) *), wildcard_status))
}

/// A container for a function parsed within a `func` attribute. Note that the
/// span of the `func` atribute is included because the syn nodes were
/// manipulated as a string and have lost therr own span information.
#[derive(Clone)]
struct DeriveFunc {
    vis: syn::Visibility,
    sig: syn::Signature,
    /// The name used to look up associations for this function. This defaults
    /// to the function name but can be overridden with `as <ident>` after the
    /// signature, which is necessary when multiple trait methods share a name
    /// (for example, several `From::from` implementations).
    assoc_name: syn::Ident,
    span: proc_macro2::Span,
    def: Option<proc_macro2::TokenStream>,
}

/// An item parsed from a `func` attribute: either an inherent function or an
/// `impl` block whose method bodies are generated by this macro.
enum DeriveItem {
    Func(DeriveFunc),
    Impl(DeriveImpl),
}

/// A trait or inherent `impl` block parsed from a `func` attribute. The methods
/// are generated in the same way as functions declared directly in a `func`
/// attribute.
struct DeriveImpl {
    unsafety: Option<Token![unsafe]>,
    generics: syn::Generics,
    trait_: Option<(Option<Token![!]>, syn::Path, Token![for])>,
    self_ty: syn::Type,
    funcs: Vec<DeriveFunc>,
}

/// An association. Contains a function ident as well as the actual tokens of
/// the VALUE (not the variant) of the association.
struct Association {
    func: syn::Ident,
    assoc: AssociationType,
}

enum AssociationType {
    Forward(syn::Expr),
    Reverse(syn::Pat),
}

/// For reverse associations, this enum keeps track of wldcard patterns. For
/// forward associations, the value is always set to "None". This is also used
/// to sort reverse associations appropriately. If more complex sorting is to
/// be implemented, updating this enum would be the best way to start.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Wildcard {
    False = 0,
    None = 1,
    True = 2,
}

impl syn::parse::Parse for DeriveFunc {
    /// Parse a function signature from an attribute
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let vis = input.parse::<syn::Visibility>()?;
        let sig = input.parse::<syn::Signature>()?;
        let span = sig.ident.span();
        let assoc_name = if input.peek(Token![as]) {
            input.parse::<Token![as]>()?;
            input.parse::<syn::Ident>()?
        } else {
            sig.ident.clone()
        };
        let def = if input.peek(syn::token::Brace) {
            let block = input.parse::<syn::Block>()?;
            Some(ToTokens::into_token_stream(block))
        } else {
            None
        };
        Ok(DeriveFunc {
            vis,
            sig,
            assoc_name,
            span,
            def,
        })
    }
}

impl syn::parse::Parse for DeriveItem {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let is_impl =
            input.peek(Token![impl]) || (input.peek(Token![unsafe]) && input.peek2(Token![impl]));
        if is_impl {
            Ok(DeriveItem::Impl(input.parse()?))
        } else {
            Ok(DeriveItem::Func(input.parse()?))
        }
    }
}

impl syn::parse::Parse for DeriveImpl {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let unsafety = input.parse::<Option<Token![unsafe]>>()?;
        input.parse::<Token![impl]>()?;
        let mut generics = input.parse::<syn::Generics>()?;
        let bang = input.parse::<Option<Token![!]>>()?;
        let first_path = input.parse::<syn::Path>()?;
        let (trait_, self_ty) = if input.peek(Token![for]) {
            let for_token = input.parse::<Token![for]>()?;
            let self_ty = input.parse::<syn::Type>()?;
            (Some((bang, first_path, for_token)), self_ty)
        } else {
            if let Some(bang) = bang {
                return Err(Error::new(
                    bang.span,
                    "`!` is only allowed in trait implementations",
                ));
            }
            (
                None,
                syn::Type::Path(syn::TypePath {
                    attrs: Vec::new(),
                    qself: None,
                    path: first_path,
                }),
            )
        };
        generics.where_clause = input.parse::<Option<syn::WhereClause>>()?;
        let content;
        braced!(content in input);
        let mut funcs = Vec::new();
        while !content.is_empty() {
            funcs.push(content.parse::<DeriveFunc>()?);
            if content.peek(Token![,]) {
                content.parse::<Token![,]>()?;
            }
        }
        Ok(DeriveImpl {
            unsafety,
            generics,
            trait_,
            self_ty,
            funcs,
        })
    }
}

struct DeriveFuncs(Vec<DeriveItem>);
impl syn::parse::Parse for DeriveFuncs {
    /// Parse a list of function signatures form an attribute
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        input.step(|cursor| {
            if let Some((_, next)) = cursor.token_tree() {
                Ok(((), next))
            } else {
                Err(cursor.error("Missing function signature"))
            }
        })?;
        let content;
        parenthesized!(content in input);
        Ok(Self(
            content
                .parse_terminated(DeriveItem::parse, Token!(,))
                .map(|parsed| parsed.into_iter().collect())?,
        ))
    }
}

/// Used to parse forward associations, which are of form Ident = Expr
struct ForwardAssocTokens(syn::Ident, syn::Expr);
impl syn::parse::Parse for ForwardAssocTokens {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let ident = input.parse()?;
        input.parse::<syn::Token!(=)>()?;
        let expr = input.parse()?;
        Ok(Self(ident, expr))
    }
}

/// Used to parse reverse associations, which are of form Ident = Pat
struct ReverseAssocTokens(syn::Ident, syn::Pat);
impl syn::parse::Parse for ReverseAssocTokens {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let ident = input.parse()?;
        input.parse::<syn::Token!(=)>()?;
        let pat = syn::Pat::parse_multi_with_leading_vert(input)?;
        Ok(Self(ident, pat))
    }
}

impl From<ForwardAssocTokens> for Association {
    fn from(val: ForwardAssocTokens) -> Self {
        Association {
            func: val.0,
            assoc: AssociationType::Forward(val.1),
        }
    }
}

impl From<ReverseAssocTokens> for Association {
    fn from(val: ReverseAssocTokens) -> Self {
        Association {
            func: val.0,
            assoc: AssociationType::Reverse(val.1),
        }
    }
}

impl Association {
    fn get_variant_assocs(variant: &Variant, is_reverse: bool) -> impl Iterator<Item = Self> + '_ {
        variant
            .attrs
            .iter()
            .filter(|assoc_attr| assoc_attr.path().is_ident(ASSOC_ATTR))
            .filter_map(move |attr| {
                if let syn::Meta::List(meta_list) = &attr.meta {
                    if is_reverse {
                        let parser = Punctuated::<ReverseAssocTokens, Token![,]>::parse_terminated;
                        parser
                            .parse2(meta_list.tokens.clone())
                            .map(|tokens| {
                                tokens
                                    .into_iter()
                                    .map(|tokens| tokens.into())
                                    .collect::<Vec<Self>>()
                            })
                            .ok()
                    } else {
                        let parser = Punctuated::<ForwardAssocTokens, Token![,]>::parse_terminated;
                        parser
                            .parse2(meta_list.tokens.clone())
                            .map(|tokens| {
                                tokens
                                    .into_iter()
                                    .map(|tokens| tokens.into())
                                    .collect::<Vec<Self>>()
                            })
                            .ok()
                    }
                } else {
                    None
                }
            })
            .flatten()
    }
}
