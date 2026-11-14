//! `#[tauri::command]` for the local server.
//!
//! The function is left as written. A wrapper deserializes JSON arguments (camelCase
//! keys, as Tauri did) and is registered with [`inventory`] so the HTTP router can
//! call it by name. `AppHandle`, `State`, `Window`, `Webview`, and `Channel` are
//! injected by the server rather than read from the body.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
  parse_macro_input, FnArg, GenericArgument, ItemFn, Pat, PathArguments, ReturnType, Type,
};

#[proc_macro_attribute]
pub fn command(attr: TokenStream, item: TokenStream) -> TokenStream {
  let _ = attr;
  let function = parse_macro_input!(item as ItemFn);
  let name = &function.sig.ident;
  let wrapper = format_ident!("__fabricator_cmd_{}", name);
  let is_async = function.sig.asyncness.is_some();
  let result_return = is_result(&function.sig.output);

  let mut bindings = Vec::new();
  let mut call_args = Vec::new();
  for arg in &function.sig.inputs {
    let FnArg::Typed(pat_type) = arg else {
      continue;
    };
    let Some(ident) = pat_ident(&pat_type.pat) else {
      return syn::Error::new_spanned(pat_type, "command arguments must be plain names")
        .to_compile_error()
        .into();
    };
    let ty = &*pat_type.ty;
    let binding = bind_arg(ident, ty);
    bindings.push(binding);
    call_args.push(quote!(#ident));
  }

  let call = if is_async {
    quote!(#name(#(#call_args),*).await)
  } else {
    quote!(#name(#(#call_args),*))
  };
  let finish = if result_return {
    quote! {
      match #call {
        Ok(value) => ::serde_json::to_value(&value).map_err(|error| error.to_string()),
        Err(error) => Err(error.to_string()),
      }
    }
  } else {
    quote!(::serde_json::to_value(&#call).map_err(|error| error.to_string()))
  };

  let expanded = quote! {
    #function

    ::tauri::inventory::submit! {
      ::tauri::CommandEntry {
        name: stringify!(#name),
        call: #wrapper,
      }
    }

    fn #wrapper(
      __ctx: ::tauri::InvokeCtx,
      __args: ::serde_json::Value,
    ) -> ::std::pin::Pin<
      Box<dyn ::std::future::Future<Output = ::std::result::Result<::serde_json::Value, ::std::string::String>> + Send>,
    > {
      Box::pin(async move {
        #(#bindings)*
        #finish
      })
    }
  };
  expanded.into()
}

fn pat_ident(pat: &Pat) -> Option<&syn::Ident> {
  match pat {
    Pat::Ident(pat) => Some(&pat.ident),
    _ => None,
  }
}

fn last_type_ident(ty: &Type) -> Option<&syn::Ident> {
  let Type::Path(path) = ty else {
    return None;
  };
  path.path.segments.last().map(|segment| &segment.ident)
}

fn is_injected(ty: &Type, name: &str) -> bool {
  last_type_ident(ty).is_some_and(|ident| ident == name)
}

fn state_inner(ty: &Type) -> Option<&Type> {
  let Type::Path(path) = ty else {
    return None;
  };
  let segment = path.path.segments.last()?;
  if segment.ident != "State" {
    return None;
  }
  let PathArguments::AngleBracketed(args) = &segment.arguments else {
    return None;
  };
  args.args.iter().find_map(|arg| match arg {
    GenericArgument::Type(ty) => Some(ty),
    _ => None,
  })
}

fn bind_arg(ident: &syn::Ident, ty: &Type) -> proc_macro2::TokenStream {
  if is_injected(ty, "AppHandle") {
    return quote! { let #ident = __ctx.app.clone(); };
  }
  if is_injected(ty, "Window") {
    return quote! { let #ident = __ctx.window(); };
  }
  if is_injected(ty, "Webview") {
    return quote! { let #ident = __ctx.webview(); };
  }
  if let Some(inner) = state_inner(ty) {
    return quote! { let #ident = __ctx.app.state::<#inner>(); };
  }
  if is_injected(ty, "Channel") {
    let key = camel(&ident.to_string());
    return quote! {
      let #ident: #ty = ::tauri::ipc::Channel::from_arg(&__ctx, __args.get(#key))?;
    };
  }
  let key = camel(&ident.to_string());
  quote! {
    let #ident: #ty = ::tauri::arg(#key, __args.get(#key))?;
  }
}

fn is_result(output: &ReturnType) -> bool {
  let ReturnType::Type(_, ty) = output else {
    return false;
  };
  let Type::Path(path) = ty.as_ref() else {
    return false;
  };
  path
    .path
    .segments
    .last()
    .is_some_and(|segment| segment.ident == "Result" || segment.ident == "AppResult")
}

fn camel(name: &str) -> String {
  let mut out = String::new();
  let mut upper = false;
  for ch in name.chars() {
    if ch == '_' {
      upper = true;
      continue;
    }
    if upper {
      out.extend(ch.to_uppercase());
      upper = false;
    } else {
      out.push(ch);
    }
  }
  out
}
