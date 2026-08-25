use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{
    parse_macro_input, Attribute, Data, DeriveInput, Expr, Fields, Ident, LitStr, Token, Type,
};

#[proc_macro_derive(Effect, attributes(effect, payload))]
pub fn derive_effect(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand(&input) {
        Ok(generated) => generated.into(),
        Err(refusal) => refusal.to_compile_error().into(),
    }
}

enum Arg {
    Name(Expr),
    Minimum(LitStr),
    Target(LitStr),
    State(Type),
    Error(Type),
    Unknown(Ident),
}

impl Parse for Arg {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let key: Ident = input.parse()?;
        input.parse::<Token![=]>()?;
        match key.to_string().as_str() {
            "name" => Ok(Arg::Name(input.parse()?)),
            "minimum" => Ok(Arg::Minimum(input.parse()?)),
            "target" => Ok(Arg::Target(input.parse()?)),
            "state" => Ok(Arg::State(input.parse()?)),
            "error" => Ok(Arg::Error(input.parse()?)),
            _ => {
                while !input.is_empty() && !input.peek(Token![,]) {
                    input.parse::<proc_macro2::TokenTree>()?;
                }
                Ok(Arg::Unknown(key))
            }
        }
    }
}

struct Header {
    name: Expr,
    minimum: LitStr,
    target: LitStr,
    state: Type,
    error: Type,
}

struct PayloadField {
    key: String,
    span: proc_macro2::Span,
    field: Ident,
}

fn expand(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let operation = &input.ident;
    let header = header(operation, &input.attrs)?;
    let fields = named_fields(operation, &input.data)?;
    let payload = payload_fields(operation, fields)?;
    let (format, arguments) = target_format(operation, &header.target, fields)?;
    let minimum = minimum(operation, &header.minimum)?;

    let name = &header.name;
    let state = &header.state;
    let error = &header.error;
    let keys = payload.iter().map(|marked| &marked.key);
    let sources = payload.iter().map(|marked| &marked.field);

    Ok(quote! {
        impl #operation {
            pub const fn descriptor() -> ::fiddle_runtime::effect::EffectDescriptor {
                ::fiddle_runtime::effect::EffectDescriptor {
                    name: #name,
                    minimum: ::fiddle_runtime::core::HumanDecisionRequirement::#minimum,
                    construct: ::fiddle_runtime::effect::build::<Self>,
                }
            }

            pub fn target(&self) -> ::std::string::String {
                ::std::format!(#format #(, self.#arguments)*)
            }
        }

        #[::fiddle_runtime::derive_support::async_trait]
        impl ::fiddle_runtime::effect::IntegrationOperation for #operation {
            type State = #state;

            type Error = #error;

            fn kind(&self) -> ::fiddle_runtime::core::EffectName {
                ::fiddle_runtime::core::EffectName::shipped(#name)
            }

            fn target(&self) -> ::std::string::String {
                Self::target(self)
            }

            fn minimum(&self) -> ::fiddle_runtime::core::HumanDecisionRequirement {
                Self::descriptor().minimum
            }

            fn payload(&self) -> ::std::string::String {
                ::fiddle_runtime::derive_support::serde_json::json!({
                    #( #keys: self.#sources ),*
                })
                .to_string()
            }

            async fn inspect(
                &self,
                ctx: &::fiddle_runtime::effect::EffectContext,
            ) -> ::std::result::Result<::std::option::Option<Self::State>, Self::Error> {
                Self::inspect(self, ctx).await
            }

            async fn apply(
                &self,
                ctx: &::fiddle_runtime::effect::EffectContext,
                authorized: &::fiddle_runtime::effect::AuthorizedEffect<Self>,
            ) -> ::std::result::Result<(), Self::Error> {
                Self::apply(self, ctx, authorized).await
            }
        }
    })
}

fn header(operation: &Ident, attrs: &[Attribute]) -> syn::Result<Header> {
    let attr = attrs
        .iter()
        .find(|attr| attr.path().is_ident("effect"))
        .ok_or_else(|| {
            syn::Error::new_spanned(
                operation,
                format!("`{operation}` derives Effect and carries no `#[effect(...)]`"),
            )
        })?;

    let args = attr.parse_args_with(Punctuated::<Arg, Token![,]>::parse_terminated)?;

    let mut name = None;
    let mut minimum = None;
    let mut target = None;
    let mut state = None;
    let mut error = None;

    for arg in args {
        match arg {
            Arg::Name(value) => name = Some(value),
            Arg::Minimum(value) => minimum = Some(value),
            Arg::Target(value) => target = Some(value),
            Arg::State(value) => state = Some(value),
            Arg::Error(value) => error = Some(value),
            Arg::Unknown(key) => {
                return Err(syn::Error::new_spanned(
                    &key,
                    format!(
                        "`{operation}`: `#[effect(...)]` knows no key `{key}`; \
                         it reads `name`, `minimum`, `target`, `state` and `error`"
                    ),
                ))
            }
        }
    }

    Ok(Header {
        name: required(operation, attr, name, "name")?,
        minimum: required(operation, attr, minimum, "minimum")?,
        target: required(operation, attr, target, "target")?,
        state: required(operation, attr, state, "state")?,
        error: required(operation, attr, error, "error")?,
    })
}

fn required<T>(operation: &Ident, attr: &Attribute, held: Option<T>, key: &str) -> syn::Result<T> {
    held.ok_or_else(|| {
        syn::Error::new_spanned(
            attr,
            format!("`{operation}`: `#[effect(...)]` states no `{key}`"),
        )
    })
}

fn named_fields<'a>(
    operation: &Ident,
    data: &'a Data,
) -> syn::Result<&'a Punctuated<syn::Field, Token![,]>> {
    match data {
        Data::Struct(held) => match &held.fields {
            Fields::Named(named) => Ok(&named.named),
            _ => Err(syn::Error::new_spanned(
                operation,
                format!("`{operation}` derives Effect and its fields are not named"),
            )),
        },
        _ => Err(syn::Error::new_spanned(
            operation,
            format!("`{operation}` derives Effect and is not a struct"),
        )),
    }
}

fn payload_fields(
    operation: &Ident,
    fields: &Punctuated<syn::Field, Token![,]>,
) -> syn::Result<Vec<PayloadField>> {
    let mut marked = Vec::new();

    for field in fields {
        let Some(attr) = field
            .attrs
            .iter()
            .find(|attr| attr.path().is_ident("payload"))
        else {
            continue;
        };
        let named = field
            .ident
            .clone()
            .expect("a named struct has named fields");
        let key = match &attr.meta {
            syn::Meta::Path(_) => named.to_string(),
            _ => renamed(operation, attr)?,
        };
        marked.push(PayloadField {
            key,
            span: named.span(),
            field: named,
        });
    }

    if marked.is_empty() {
        return Err(syn::Error::new_spanned(
            operation,
            format!("`{operation}` derives Effect and marks no field `#[payload]`"),
        ));
    }

    marked.sort_by(|left, right| left.key.cmp(&right.key));

    let mut seen = std::collections::BTreeSet::new();
    for field in &marked {
        if !seen.insert(field.key.clone()) {
            return Err(syn::Error::new(
                field.span,
                format!(
                    "`{operation}`: two payload fields both write the key `{}`",
                    field.key
                ),
            ));
        }
    }

    Ok(marked)
}

fn renamed(operation: &Ident, attr: &Attribute) -> syn::Result<String> {
    let mut key = None;
    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("rename") {
            key = Some(meta.value()?.parse::<LitStr>()?.value());
            return Ok(());
        }
        Err(meta.error(format!(
            "`{operation}`: `#[payload(...)]` knows no key `{}`; it reads `rename`",
            meta.path
                .get_ident()
                .map(Ident::to_string)
                .unwrap_or_default()
        )))
    })?;
    key.ok_or_else(|| {
        syn::Error::new_spanned(
            attr,
            format!("`{operation}`: `#[payload(...)]` states no `rename`"),
        )
    })
}

fn target_format(
    operation: &Ident,
    literal: &LitStr,
    fields: &Punctuated<syn::Field, Token![,]>,
) -> syn::Result<(LitStr, Vec<Ident>)> {
    let spelled = literal.value();
    let mut format = String::new();
    let mut arguments = Vec::new();
    let mut rest = spelled.as_str();

    while let Some(open) = rest.find(['{', '}']) {
        format.push_str(&rest[..open]);
        rest = &rest[open..];

        if rest.starts_with("{{") || rest.starts_with("}}") {
            format.push_str(&rest[..2]);
            rest = &rest[2..];
            continue;
        }

        if rest.starts_with('}') {
            return Err(syn::Error::new(
                literal.span(),
                format!("`{operation}`: `target` closes a placeholder it never opened"),
            ));
        }

        let close = rest.find('}').ok_or_else(|| {
            syn::Error::new(
                literal.span(),
                format!("`{operation}`: `target` opens a placeholder it never closes"),
            )
        })?;
        let placeholder = &rest[1..close];
        rest = &rest[close + 1..];

        let held = fields
            .iter()
            .filter_map(|field| field.ident.as_ref())
            .find(|field| *field == placeholder)
            .ok_or_else(|| {
                syn::Error::new(
                    literal.span(),
                    format!(
                        "`{operation}`: `target` reads `{{{placeholder}}}` and \
                         `{operation}` has no field `{placeholder}`"
                    ),
                )
            })?;

        format.push_str("{}");
        arguments.push(held.clone());
    }

    format.push_str(rest);
    Ok((LitStr::new(&format, literal.span()), arguments))
}

fn minimum(operation: &Ident, literal: &LitStr) -> syn::Result<Ident> {
    match literal.value().as_str() {
        "automatic" => Ok(Ident::new("Automatic", literal.span())),
        "human" => Ok(Ident::new("Human", literal.span())),
        other => Err(syn::Error::new(
            literal.span(),
            format!(
                "`{operation}`: `minimum` reads `{other}` and \
                 a human decision is either `automatic` or `human`"
            ),
        )),
    }
}
