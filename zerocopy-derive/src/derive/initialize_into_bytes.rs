use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Data, DataEnum, DataStruct, DataUnion, Error, Type};

use crate::{
    repr::{EnumRepr, StructUnionRepr},
    util::{
        generate_tag_enum, Ctx, DataExt, FieldBounds, ImplBlockBuilder, PaddingCheck, Trait,
        TraitBound,
    },
    SelfBounds,
};

pub(crate) fn derive_initialize_into_bytes(
    ctx: &Ctx,
    top_level: Trait,
) -> Result<TokenStream, Error> {
    try_gen_trivial_initialize_into_bytes(ctx, top_level).map(Ok).unwrap_or_else(|| {
        match &ctx.ast.data {
            Data::Struct(strct) => derive_initialize_into_bytes_struct(ctx, strct),
            Data::Enum(enm) => derive_initialize_into_bytes_enum(ctx, enm),
            Data::Union(unn) => derive_initialize_into_bytes_union(ctx, unn),
        }
    })
}

fn try_gen_trivial_initialize_into_bytes(
    ctx: &Ctx,
    top_level: Trait,
) -> Option<proc_macro2::TokenStream> {
    // If the top-level trait is `IntoBytes`, `IntoBytes` derive will fail
    // compilation if `Self` is not actually soundly `IntoBytes`, and so we can
    // rely on that for our `zeroize` impl. It's plausible that we could
    // make changes - or Rust could make changes (such as the "trivial bounds"
    // language feature) - that make this no longer true. To hedge against
    // these, we include an explicit `Self: IntoBytes` check in the generated
    // `is_bit_valid`, which is bulletproof.
    //
    // If `ctx.skip_on_error` is true, we can't rely on the `IntoBytes` derive
    // to fail compilation if `Self` is not actually soundly `IntoBytes`.
    if matches!(top_level, Trait::IntoBytes)
        && ctx.ast.generics.params.is_empty()
        && !ctx.skip_on_error
    {
        let zerocopy_crate = &ctx.zerocopy_crate;
        let core = ctx.core_path();

        Some(
            ImplBlockBuilder::new(
                ctx,
                &ctx.ast.data,
                Trait::InitializeIntoBytes,
                FieldBounds::ALL_SELF,
            )
            .self_type_trait_bounds(SelfBounds::All(&[Trait::IntoBytes]))
            .inner_extras(quote!(
                // SAFETY: See inline.
                #[inline(always)]
                fn initialize_padding(ptr: #zerocopy_crate::Ptr<'_, Self, (
                    #zerocopy_crate::invariant::Exclusive,
                    #zerocopy_crate::invariant::Unaligned,
                    #zerocopy_crate::invariant::Valid)>
                ) {
                    if false {
                        fn assert_is_into_bytes<T>()
                        where
                            T: #zerocopy_crate::IntoBytes,
                            T: ?#core::marker::Sized,
                        {
                        }

                        assert_is_into_bytes::<Self>();
                    }
                }
            ))
            .build(),
        )
    } else {
        None
    }
}

fn derive_initialize_into_bytes_struct(
    ctx: &Ctx,
    strct: &DataStruct,
) -> Result<TokenStream, Error> {
    let zerocopy_crate = &ctx.zerocopy_crate;
    let core = ctx.core_path();
    Ok(ImplBlockBuilder::new(ctx, &ctx.ast.data, Trait::InitializeIntoBytes, FieldBounds::ALL_SELF)
        .inner_extras(quote!(
            // SAFETY: See inline.
            #[inline(always)]
            fn initialize_padding(ptr: #zerocopy_crate::Ptr<'_, Self, (
                #zerocopy_crate::invariant::Exclusive,
                #zerocopy_crate::invariant::Unaligned,
                #zerocopy_crate::invariant::Valid)>
            ) {
                todo!()
            }
        ))
        .build())
}

fn derive_initialize_into_bytes_enum(ctx: &Ctx, enm: &DataEnum) -> Result<TokenStream, Error> {
    todo!()
}

fn derive_initialize_into_bytes_union(ctx: &Ctx, unn: &DataUnion) -> Result<TokenStream, Error> {
    todo!()
}
