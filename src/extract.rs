use crate::error::ErrorRecord;
use crate::options::MacroOpts;
use crate::signature::TestFnSignature;

use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::punctuated::Punctuated;
use syn::Token;
use syn::{
    AngleBracketedGenericArguments, AttrStyle, Attribute, Error, GenericArgument, GenericParam,
    Generics, Ident, Item, ItemFn, ItemMod, ReturnType,
};

#[derive(Default)]
pub struct Tests {
    pub test_fns: Vec<TestFn>,
}

pub struct TestFn {
    pub test_attrs: Vec<Attribute>,
    pub asyncness: Option<Token![async]>,
    pub unsafety: Option<Token![unsafe]>,
    pub ident: Ident,
    pub output: ReturnType,
    pub sig: TestFnSignature,
}

impl Tests {
    pub fn try_extract<'ast>(
        opts: &MacroOpts,
        ast: &'ast mut ItemMod,
    ) -> syn::Result<(Self, &'ast mut Vec<Item>)> {
        if ast.content.is_none() {
            return Err(Error::new_spanned(ast, "only inline modules are supported"));
        }
        let items = &mut ast.content.as_mut().unwrap().1;
        let (tests, errors) = Self::extract_recording_errors(opts, items);
        errors.check()?;
        Ok((tests, items))
    }

    fn extract_recording_errors<'ast>(
        opts: &MacroOpts,
        items: &'ast mut Vec<Item>,
    ) -> (Self, ErrorRecord) {
        let mut errors = ErrorRecord::default();
        let mut tests = Tests::default();
        let mut mod_wide_generic_arity = None;
        for item in items.iter_mut() {
            if let Item::Fn(item) = item {
                if let Some(test_attrs) = extract_test_attrs(opts, item) {
                    let sig = match TestFnSignature::try_build(item) {
                        Ok(sig) => sig,
                        Err(e) => {
                            errors.add_error(e);
                            continue;
                        }
                    };
                    let fn_generic_arity = generic_arity(&item.sig.generics);
                    match mod_wide_generic_arity {
                        None => {
                            mod_wide_generic_arity = Some(fn_generic_arity);
                        }
                        Some(n) => {
                            if fn_generic_arity != n {
                                errors.add_error(Error::new_spanned(
                                    &item.sig.generics,
                                    format!(
                                        "test function `{}` has {} generic parameters \
                                        while others in the same module have {}",
                                        item.sig.ident, fn_generic_arity, n
                                    ),
                                ));
                                continue;
                            }
                        }
                    }
                    tests.test_fns.push(TestFn {
                        test_attrs,
                        asyncness: item.sig.asyncness,
                        unsafety: item.sig.unsafety,
                        ident: item.sig.ident.clone(),
                        output: item.sig.output.clone(),
                        sig,
                    });
                }
            }
        }
        (tests, errors)
    }
}

fn extract_test_attrs(opts: &MacroOpts, item: &mut ItemFn) -> Option<Vec<Attribute>> {
    let mut test_attrs = Vec::new();
    let mut pos = 0;
    while pos < item.attrs.len() {
        let attr = &item.attrs[pos];
        if opts.is_test_attr(&attr) {
            test_attrs.push(item.attrs.remove(pos));
            continue;
        }
        pos += 1;
    }
    if test_attrs.is_empty() {
        None
    } else {
        for attr in &item.attrs {
            if opts.is_copied_attr(&attr) {
                test_attrs.push(attr.clone());
            }
        }
        Some(test_attrs)
    }
}

fn generic_arity(generics: &Generics) -> usize {
    generics
        .params
        .iter()
        .filter(|param| match param {
            GenericParam::Type(_) | GenericParam::Const(_) => true,
            GenericParam::Lifetime(_) => false,
        })
        .count()
}

pub struct InstArguments(Punctuated<GenericArgument, Token![,]>);

impl InstArguments {
    pub fn try_extract(item: &mut ItemMod) -> syn::Result<Option<Self>> {
        for (pos, attr) in item.attrs.iter().enumerate() {
            if attr.path.is_ident("instantiate_tests") {
                match attr.style {
                    AttrStyle::Outer => {}
                    AttrStyle::Inner(_) => {
                        return Err(Error::new_spanned(attr, "cannot be an inner attribute"))
                    }
                };
                let AngleBracketedGenericArguments { args, .. } = attr.parse_args()?;
                item.attrs.remove(pos);
                return Ok(Some(InstArguments(args)));
            }
        }
        Ok(None)
    }
}

impl ToTokens for InstArguments {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens)
    }
}
