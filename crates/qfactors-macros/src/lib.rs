use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{
    Error, Expr, ExprArray, ExprLit, FnArg, GenericArgument, Ident, ItemFn, Lit, Meta, Pat,
    PathArguments, ReturnType, Token, Type, TypePath,
};

#[proc_macro_attribute]
pub fn factor(attr: TokenStream, item: TokenStream) -> TokenStream {
    match expand_factor(attr.into(), item.into()) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn expand_factor(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    let args = FactorArgs::parse(attr)?;
    let function: ItemFn = syn::parse2(item)?;
    let analysis = FunctionAnalysis::new(&function, &args)?;

    Ok(generate_factor(function, analysis))
}

enum WindowArgs {
    Single(usize),
    Multi(Vec<usize>),
}

struct FactorArgs {
    windows: WindowArgs,
    outputs: Option<Vec<String>>,
}

impl FactorArgs {
    fn parse(tokens: TokenStream2) -> syn::Result<Self> {
        let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
        let metas = parser.parse2(tokens)?;
        let mut window = None;
        let mut windows = None;
        let mut outputs = None;

        for meta in metas {
            let Meta::NameValue(name_value) = meta else {
                return Err(Error::new_spanned(
                    meta,
                    "expected name-value factor attribute",
                ));
            };

            if name_value.path.is_ident("window") {
                if window.is_some() {
                    return Err(Error::new_spanned(name_value, "duplicate `window`"));
                }
                window = Some(parse_window(&name_value.value)?);
            } else if name_value.path.is_ident("windows") {
                if windows.is_some() {
                    return Err(Error::new_spanned(name_value, "duplicate `windows`"));
                }
                windows = Some(parse_windows(&name_value.value)?);
            } else if name_value.path.is_ident("outputs") {
                if outputs.is_some() {
                    return Err(Error::new_spanned(name_value, "duplicate `outputs`"));
                }
                outputs = Some(parse_outputs(&name_value.value)?);
            } else if name_value.path.is_ident("params") {
                return Err(Error::new_spanned(
                    name_value,
                    "`params` are not supported until Phase 6",
                ));
            } else {
                return Err(Error::new_spanned(
                    name_value.path,
                    "unsupported factor attribute",
                ));
            }
        }

        let windows = match (window, windows) {
            (Some(window), None) => WindowArgs::Single(window),
            (None, Some(windows)) => WindowArgs::Multi(windows),
            (Some(_), Some(_)) => {
                return Err(Error::new(
                    proc_macro2::Span::call_site(),
                    "`window` and `windows` cannot be used together",
                ));
            }
            (None, None) => {
                return Err(Error::new(
                    proc_macro2::Span::call_site(),
                    "expected `window = N` or `windows = [N, ...]`",
                ));
            }
        };

        Ok(Self { windows, outputs })
    }
}

fn parse_window(expr: &Expr) -> syn::Result<usize> {
    let Expr::Lit(ExprLit {
        lit: Lit::Int(lit), ..
    }) = expr
    else {
        return Err(Error::new_spanned(
            expr,
            "`window` must be a positive integer",
        ));
    };

    let value = lit.base10_parse::<usize>()?;
    if value == 0 {
        return Err(Error::new_spanned(
            expr,
            "`window` must be greater than zero",
        ));
    }
    Ok(value)
}

fn parse_windows(expr: &Expr) -> syn::Result<Vec<usize>> {
    let Expr::Array(ExprArray { elems, .. }) = expr else {
        return Err(Error::new_spanned(
            expr,
            "`windows` must be an array of positive integers",
        ));
    };
    if elems.is_empty() {
        return Err(Error::new_spanned(expr, "`windows` cannot be empty"));
    }

    elems.iter().map(parse_window).collect()
}

fn parse_outputs(expr: &Expr) -> syn::Result<Vec<String>> {
    let Expr::Array(ExprArray { elems, .. }) = expr else {
        return Err(Error::new_spanned(
            expr,
            "`outputs` must be an array of string literals",
        ));
    };
    if elems.is_empty() {
        return Err(Error::new_spanned(expr, "`outputs` cannot be empty"));
    }

    elems
        .iter()
        .map(|expr| {
            let Expr::Lit(ExprLit {
                lit: Lit::Str(lit), ..
            }) = expr
            else {
                return Err(Error::new_spanned(
                    expr,
                    "`outputs` entries must be string literals",
                ));
            };

            let value = lit.value();
            if value.is_empty() {
                return Err(Error::new_spanned(lit, "`outputs` entries cannot be empty"));
            }
            Ok(value)
        })
        .collect()
}

struct FunctionAnalysis {
    kernel_name: String,
    inputs: Vec<InputSpec>,
    output_names: Vec<String>,
    output_count: usize,
    returns_result: bool,
    factors: Vec<GeneratedFactor>,
}

struct InputSpec {
    name: String,
    dtype: Ident,
    accessor: Ident,
}

struct GeneratedFactor {
    factor_name: String,
    window: usize,
    descriptor_ident: Ident,
    register_ident: Ident,
}

enum OutputShape {
    Single,
    Tuple(usize),
}

struct ReturnSpec {
    shape: OutputShape,
    returns_result: bool,
}

impl FunctionAnalysis {
    fn new(function: &ItemFn, args: &FactorArgs) -> syn::Result<Self> {
        let signature = &function.sig;
        if signature.constness.is_some() {
            return Err(Error::new_spanned(
                signature.constness,
                "factor functions cannot be const",
            ));
        }
        if signature.asyncness.is_some() {
            return Err(Error::new_spanned(
                signature.asyncness,
                "factor functions cannot be async",
            ));
        }
        if signature.unsafety.is_some() {
            return Err(Error::new_spanned(
                signature.unsafety,
                "factor functions cannot be unsafe",
            ));
        }
        if !signature.generics.params.is_empty() {
            return Err(Error::new_spanned(
                &signature.generics,
                "factor functions cannot be generic",
            ));
        }
        if signature.variadic.is_some() {
            return Err(Error::new_spanned(
                &signature.variadic,
                "factor functions cannot be variadic",
            ));
        }

        let mut inputs = Vec::with_capacity(signature.inputs.len());
        for arg in &signature.inputs {
            inputs.push(parse_input(arg)?);
        }

        let return_spec = parse_return_type(&signature.output)?;
        let (output_names, output_count) =
            resolve_output_names(&signature.ident, args.outputs.as_ref(), &return_spec.shape)?;
        let factors = factor_names(&signature.ident, &args.windows)?;

        Ok(Self {
            kernel_name: signature.ident.to_string(),
            inputs,
            output_names,
            output_count,
            returns_result: return_spec.returns_result,
            factors,
        })
    }
}

fn parse_input(arg: &FnArg) -> syn::Result<InputSpec> {
    let FnArg::Typed(typed) = arg else {
        return Err(Error::new_spanned(arg, "factor methods are not supported"));
    };

    let Pat::Ident(pattern) = typed.pat.as_ref() else {
        return Err(Error::new_spanned(
            &typed.pat,
            "factor input must use an identifier pattern",
        ));
    };
    if pattern.subpat.is_some() {
        return Err(Error::new_spanned(
            pattern,
            "factor input must use a plain identifier",
        ));
    }

    let (dtype, accessor) = parse_slice_type(&typed.ty)?;
    Ok(InputSpec {
        name: pattern.ident.to_string(),
        dtype,
        accessor,
    })
}

fn parse_slice_type(ty: &Type) -> syn::Result<(Ident, Ident)> {
    let Type::Reference(reference) = ty else {
        return Err(Error::new_spanned(
            ty,
            "factor inputs must be typed slices like `&[f64]`",
        ));
    };
    if reference.mutability.is_some() {
        return Err(Error::new_spanned(
            reference,
            "factor inputs cannot be mutable",
        ));
    }

    let Type::Slice(slice) = reference.elem.as_ref() else {
        return Err(Error::new_spanned(
            &reference.elem,
            "factor inputs must be typed slices like `&[f64]`",
        ));
    };

    let Type::Path(TypePath { qself: None, path }) = slice.elem.as_ref() else {
        return Err(Error::new_spanned(
            &slice.elem,
            "factor input dtype must be `f64`, `u32`, or `i64`",
        ));
    };
    let Some(ident) = path.get_ident() else {
        return Err(Error::new_spanned(
            path,
            "factor input dtype must be `f64`, `u32`, or `i64`",
        ));
    };

    match ident.to_string().as_str() {
        "f64" => Ok((format_ident!("F64"), format_ident!("f64"))),
        "u32" => Ok((format_ident!("U32"), format_ident!("u32"))),
        "i64" => Ok((format_ident!("I64"), format_ident!("i64"))),
        _ => Err(Error::new_spanned(
            ident,
            "factor input dtype must be `f64`, `u32`, or `i64`",
        )),
    }
}

fn parse_return_type(output: &ReturnType) -> syn::Result<ReturnSpec> {
    let ReturnType::Type(_, ty) = output else {
        return Err(Error::new_spanned(
            output,
            "factor functions must return `f64`, `(f64, ...)`, or `Result<T>`",
        ));
    };

    if let Some(inner) = result_inner(ty)? {
        let shape = parse_output_shape(inner)?;
        Ok(ReturnSpec {
            shape,
            returns_result: true,
        })
    } else {
        let shape = parse_output_shape(ty)?;
        Ok(ReturnSpec {
            shape,
            returns_result: false,
        })
    }
}

fn result_inner(ty: &Type) -> syn::Result<Option<&Type>> {
    let Type::Path(TypePath { qself: None, path }) = ty else {
        return Ok(None);
    };
    let Some(segment) = path.segments.last() else {
        return Ok(None);
    };
    if segment.ident != "Result" {
        return Ok(None);
    }

    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return Err(Error::new_spanned(
            ty,
            "`Result` return type must be `Result<T>`",
        ));
    };
    if args.args.len() != 1 {
        return Err(Error::new_spanned(
            ty,
            "`Result` return type must be `Result<T>`",
        ));
    }

    let Some(GenericArgument::Type(inner)) = args.args.first() else {
        return Err(Error::new_spanned(
            ty,
            "`Result` return type must be `Result<T>`",
        ));
    };
    Ok(Some(inner))
}

fn parse_output_shape(ty: &Type) -> syn::Result<OutputShape> {
    if is_f64_type(ty) {
        return Ok(OutputShape::Single);
    }

    let Type::Tuple(tuple) = ty else {
        return Err(Error::new_spanned(
            ty,
            "factor return type must be `f64`, `(f64, ...)`, or `Result<T>`",
        ));
    };
    if tuple.elems.len() < 2 {
        return Err(Error::new_spanned(
            tuple,
            "tuple factor output must contain at least two values",
        ));
    }
    for elem in &tuple.elems {
        if !is_f64_type(elem) {
            return Err(Error::new_spanned(
                elem,
                "factor outputs must be `f64` values",
            ));
        }
    }

    Ok(OutputShape::Tuple(tuple.elems.len()))
}

fn is_f64_type(ty: &Type) -> bool {
    let Type::Path(TypePath { qself: None, path }) = ty else {
        return false;
    };
    path.is_ident("f64")
}

fn resolve_output_names(
    kernel_ident: &Ident,
    attr_outputs: Option<&Vec<String>>,
    shape: &OutputShape,
) -> syn::Result<(Vec<String>, usize)> {
    match shape {
        OutputShape::Single => {
            let output_names = attr_outputs
                .cloned()
                .unwrap_or_else(|| vec![kernel_ident.to_string()]);
            if output_names.len() != 1 {
                return Err(Error::new_spanned(
                    kernel_ident,
                    "single-output factors must have exactly one output name",
                ));
            }
            Ok((output_names, 1))
        }
        OutputShape::Tuple(count) => {
            let Some(output_names) = attr_outputs.cloned() else {
                return Err(Error::new_spanned(
                    kernel_ident,
                    "multi-output factors must define `outputs = [...]`",
                ));
            };
            if output_names.len() != *count {
                return Err(Error::new_spanned(
                    kernel_ident,
                    "`outputs` length must match tuple return length",
                ));
            }
            Ok((output_names, *count))
        }
    }
}

fn factor_names(kernel_ident: &Ident, windows: &WindowArgs) -> syn::Result<Vec<GeneratedFactor>> {
    let kernel_name = kernel_ident.to_string();
    let generated = match windows {
        WindowArgs::Single(window) => {
            vec![GeneratedFactor {
                factor_name: kernel_name.clone(),
                window: *window,
                descriptor_ident: format_ident!("__qfactors_{}_descriptor", kernel_ident),
                register_ident: format_ident!(
                    "__QFACTORS_REGISTER_{}",
                    kernel_name.to_ascii_uppercase()
                ),
            }]
        }
        WindowArgs::Multi(windows) => windows
            .iter()
            .map(|window| GeneratedFactor {
                factor_name: format!("{kernel_name}_{window}"),
                window: *window,
                descriptor_ident: format_ident!(
                    "__qfactors_{}_{}_descriptor",
                    kernel_ident,
                    window
                ),
                register_ident: format_ident!(
                    "__QFACTORS_REGISTER_{}_{}",
                    kernel_name.to_ascii_uppercase(),
                    window
                ),
            })
            .collect(),
    };

    Ok(generated)
}

fn generate_factor(function: ItemFn, analysis: FunctionAnalysis) -> TokenStream2 {
    let kernel_ident = &function.sig.ident;
    let inputs_ident = format_ident!("__qfactors_{}_inputs", kernel_ident);
    let outputs_ident = format_ident!("__qfactors_{}_outputs", kernel_ident);
    let compute_ident = format_ident!("__qfactors_{}_compute", kernel_ident);

    let input_count = analysis.inputs.len();
    let input_names = analysis.inputs.iter().map(|input| &input.name);
    let input_dtypes = analysis.inputs.iter().map(|input| &input.dtype);
    let output_count = analysis.output_count;
    let output_names = analysis.output_names.iter();

    let output_indices = 0..analysis.output_count;
    let output_vecs = (0..analysis.output_count)
        .map(|idx| format_ident!("__qfactors_output_{idx}"))
        .collect::<Vec<_>>();
    let output_values = (0..analysis.output_count)
        .map(|idx| format_ident!("__qfactors_value_{idx}"))
        .collect::<Vec<_>>();
    let input_locals = (0..analysis.inputs.len())
        .map(|idx| format_ident!("__qfactors_input_{idx}"))
        .collect::<Vec<_>>();
    let input_accessors = analysis.inputs.iter().map(|input| &input.accessor);
    let input_indices = 0..analysis.inputs.len();

    let call_args = input_locals
        .iter()
        .map(|ident| quote! { &#ident[__qfactors_range.clone()] });
    let kernel_call = quote! { #kernel_ident(#(#call_args),*) };
    let kernel_call = if analysis.returns_result {
        quote! { #kernel_call? }
    } else {
        kernel_call
    };

    let assign_outputs = if analysis.output_count == 1 {
        let output_vec = &output_vecs[0];
        quote! {
            #output_vec[__qfactors_group_idx] = __qfactors_kernel_output;
        }
    } else {
        quote! {
            let (#(#output_values),*) = __qfactors_kernel_output;
            #(
                #output_vecs[__qfactors_group_idx] = #output_values;
            )*
        }
    };

    let descriptor_fns = analysis.factors.iter().map(|factor| {
        let descriptor_ident = &factor.descriptor_ident;
        let register_ident = &factor.register_ident;
        let factor_name = &factor.factor_name;
        let kernel_name = &analysis.kernel_name;
        let window = factor.window;

        quote! {
            fn #descriptor_ident() -> ::qfactors_core::FactorDescriptor {
                ::qfactors_core::FactorDescriptor {
                    factor_name: #factor_name,
                    kernel_name: #kernel_name,
                    window: #window,
                    inputs: &#inputs_ident,
                    outputs: &#outputs_ident,
                    param_set: None,
                    params: &[],
                    compute: #compute_ident,
                }
            }

            #[linkme::distributed_slice(qfactors_core::registry::FACTOR_DESCRIPTORS)]
            static #register_ident: fn() -> ::qfactors_core::FactorDescriptor = #descriptor_ident;
        }
    });

    quote! {
        #function

        #[allow(non_upper_case_globals)]
        static #inputs_ident: [::qfactors_core::ColumnSpec; #input_count] = [
            #(
                ::qfactors_core::ColumnSpec {
                    name: #input_names,
                    dtype: ::qfactors_core::DType::#input_dtypes,
                },
            )*
        ];

        #[allow(non_upper_case_globals)]
        static #outputs_ident: [::qfactors_core::ColumnSpec; #output_count] = [
            #(
                ::qfactors_core::ColumnSpec {
                    name: #output_names,
                    dtype: ::qfactors_core::DType::F64,
                },
            )*
        ];

        fn #compute_ident(
            columns: &::qfactors_core::ColumnStore<'_>,
            ranges: &[::std::option::Option<::std::ops::Range<usize>>],
            factor: &::qfactors_core::ResolvedFactor<'_>,
        ) -> ::qfactors_core::Result<::qfactors_core::FactorResult> {
            #(
                let #input_locals = columns.#input_accessors(&factor.input_columns[#input_indices])?;
            )*
            #(
                let mut #output_vecs = vec![f64::NAN; ranges.len()];
            )*

            for (__qfactors_group_idx, __qfactors_range_opt) in ranges.iter().enumerate() {
                if let Some(__qfactors_range) = __qfactors_range_opt {
                    let __qfactors_kernel_output = #kernel_call;
                    #assign_outputs
                }
            }

            Ok(vec![
                #(
                    ::polars::prelude::Column::new(
                        factor.output_columns[#output_indices].clone().into(),
                        #output_vecs,
                    ),
                )*
            ])
        }

        #(#descriptor_fns)*
    }
}
