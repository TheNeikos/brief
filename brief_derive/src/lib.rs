extern crate proc_macro;
use proc_macro::TokenStream as TStream;
use proc_macro2::TokenStream;

use quote::{quote, quote_spanned, format_ident};
use syn::{parse_macro_input, spanned::Spanned, DeriveInput};

#[proc_macro_attribute]
pub fn bot(_attr: TStream, item: TStream) -> TStream {
    let input = parse_macro_input!(item as DeriveInput);

    let expanded = match bot_impl(input) {
        Ok(out) => out,
        Err(e) => e,
    };

    TStream::from(expanded)
}

fn error<S: syn::spanned::Spanned>(span: &S, err: &str) -> proc_macro2::TokenStream {
    quote_spanned!(span.span()=>compile_error!(#err);)
}

fn bot_impl(input: syn::DeriveInput) -> Result<TokenStream, TokenStream> {
    if input.generics.lt_token.is_some()
        || input.generics.gt_token.is_some()
        || input.generics.where_clause.is_some()
    {
        return Err(error(
            &input,
            "Generics on the struct are not supported right now.",
        ));
    }

    let data = match &input.data {
        syn::Data::Struct(data) => data,
        _ => return Err(error(&input, "Only structs are supported.")),
    };

    let fields = match &data.fields {
        syn::Fields::Named(fields) => fields,
        syn::Fields::Unnamed(_) | syn::Fields::Unit => {
            return Err(error(&input, "Only named struct fields are supported."))
        }
    };

    let fields = fields
        .named
        .iter()
        .map(|f| {
            let types = f
                .attrs
                .iter()
                .flat_map(|a| {
                    let brief_segment: syn::PathSegment =
                        syn::Ident::new("brief", proc_macro2::Span::call_site()).into();
                    if let Some(segment) = a.path.segments.first() {
                        if segment != &brief_segment {
                            return None;
                        }
                        let cmd_segment: syn::PathSegment =
                            syn::Ident::new("command", proc_macro2::Span::call_site()).into();
                        if let Some(segment) = a.path.segments.last() {
                            if segment == &cmd_segment {
                                let alternate_name = syn::parse(a.tokens.clone().into()).ok();
                                return Some(BriefType::Command(alternate_name));
                            }
                        }
                        let action_segment: syn::PathSegment =
                            syn::Ident::new("action", proc_macro2::Span::call_site()).into();
                        if let Some(segment) = a.path.segments.last() {
                            if segment == &action_segment {
                                let alternate_name = syn::parse(a.tokens.clone().into()).ok();
                                return Some(BriefType::Callback(alternate_name));
                            }
                        }
                    }
                    None
                })
                .collect::<Vec<_>>();
            (f, types)
        })
        .collect::<Vec<_>>();

    let erroneous_fields = fields
        .iter()
        .filter(|(_, t)| t.len() != 1)
        .collect::<Vec<_>>();
    if !erroneous_fields.is_empty() {
        let errs = erroneous_fields.iter().map(|(f, _)| error(&f.ident.as_ref().unwrap(), "Every field needs to have exactly one brief attribute, detailing what its purpose is")).collect::<Vec<_>>();
        return Err(quote!(#(#errs)*));
    }

    // `fields` now contains fields and their supposed brief type. Let's type check that now

    let checks = fields.iter().map(|(f, t)| {
        let ident = format_ident!("_Assert{}IsCorrectType", f.ident.as_ref().unwrap());
        let ty = &f.ty;
        let to_check = match t[0] {
            BriefType::Command(_) => quote!(#ty: ::brief::BotCommand),
            BriefType::Callback(_) =>  quote!(#ty: ::brief::BotAction),
        };
        quote_spanned!(f.ty.span()=> #[allow(non_camel_case_types)] struct #ident where #to_check;)
    });

    let attrs = &input.attrs;
    let vis = &input.vis;
    let ident = &input.ident;
    let struct_fields = fields.iter().map(|(f, _)| f.ident.as_ref().unwrap());
    let tys = fields.iter().map(|(f, _)| &f.ty);

    let cmds = fields.iter().filter(|(_, t)| match t[0] { BriefType::Command(_) => true, _ => false });

    let cmd_names = cmds.clone().map(|(f, t)| match &t[0] {
        BriefType::Command(Some(name)) => format!("/{}", name.0),
        BriefType::Command(None) => format!("/{}", f.ident.as_ref().unwrap()),
        _ => unreachable!(),
    });

    let cmd_types = cmds.clone().map(|(f, _)| &f.ty);

    let cmd_fields = cmds.clone().map(|(f, _)| &f.ident);

    // Callbacks

    let callbacks = fields.iter().filter(|(_, t)| match t[0] { BriefType::Callback(_) => true, _ => false });

    let callback_structs = callbacks.clone().map(|(f, _)| &f.ty);

    let callback_names = callbacks.clone().map(|(f, t)| match &t[0] {
        BriefType::Callback(Some(name)) => name.0.to_string(),
        BriefType::Callback(None) => f.ident.as_ref().unwrap().to_string(),
        _ => unreachable!(),
    }).collect::<Vec<_>>();

    let callback_types = callbacks.clone().map(|(f, _)| &f.ty);

    let callback_fields = callbacks.clone().map(|(f, _)| &f.ident);

    let implementation = quote! {
        #(#attrs)*
        #vis struct #ident {
            #(#struct_fields : #tys),*
        }

        #[brief::async_trait]
        impl ::brief::TelegramBot for #ident {
            async fn handle_command(
                &self,
                ctx: &::brief::Context<'_>,
                cmd: &str,
                args: Option<&str>,
                text: &str,
                message: &::brief::tg::Message,
            ) -> Result<::brief::Propagate, ::brief::BriefError> {

                match cmd {
                    #(#cmd_names => {
                        <#cmd_types as ::brief::BotCommand>::handle(&self.#cmd_fields, ctx, message, args, text).await?;
                    })*
                    _ => (),
                };

                Ok(::brief::Propagate::Stop)
            }

            async fn handle_callback(
                &self,
                ctx: &::brief::Context<'_>,
                callback: &::brief::tg::CallbackQuery,
            ) -> Result<(), ::brief::BriefError> {
                let mut data_args = callback.data.splitn(2, '#').collect::<Vec<_>>();

                if data_args.len() < 1 {
                    return Ok(());
                } 

                let name = data_args.remove(0);

                let arg = if !data_args.is_empty() { 
                    Some(String::from(data_args.remove(0)))
                } else {
                    None
                };

                println!("{:?}, {:?}", name, arg);

                match name {
                    #(#callback_names => {
                        <#callback_types as ::brief::BotAction>::handle(&self.#callback_fields, ctx, callback, arg).await?;
                    })*
                    _ => (),
                }

                Ok(())
            }

        }

        #(impl #callback_structs {
            fn callback_data<'a, T: Into<Option<&'a str>>>(data: T) -> String {
                format!("{}#{}", #callback_names, data.into().map(|s| s.as_ref()).unwrap_or(""))
            }
        })*
    };

    println!("{}", implementation);

    Ok(quote!(
        #(#checks)*

        #implementation
    ))
}

#[derive(Debug, PartialEq)]
enum BriefType {
    Command(Option<CommandName>),
    Callback(Option<CommandName>),
}

#[derive(Debug, PartialEq)]
struct CommandName(String);

impl syn::parse::Parse for CommandName {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(CommandName({
            let _: syn::Token![=] = input.parse()?;

            let text: syn::LitStr = input.parse()?;
            text.value()
        }))
    }
}
