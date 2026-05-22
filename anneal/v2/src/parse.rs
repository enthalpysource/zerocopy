// Copyright 2026 The Fuchsia Authors
//
// Licensed under the 2-Clause BSD License <LICENSE-BSD or
// https://opensource.org/license/bsd-2-clause>, Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>, or the MIT
// license <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your option.
// This file may not be copied, modified, or distributed except according to
// those terms.

pub mod hkd {
    pub struct Safe;
    pub trait ThreadSafety {}
    impl ThreadSafety for Safe {}
    pub struct Local;
    impl ThreadSafety for Local {}
}

#[derive(Clone, Debug)]
pub struct AnnealDecorated<T, B> {
    pub item: T,
    pub anneal: B,
}

#[derive(Clone, Debug)]
pub enum FunctionItem<M> {
    Free(std::marker::PhantomData<M>),
}

impl<M> FunctionItem<M> {
    pub fn name(&self) -> &str {
        ""
    }
}

#[derive(Clone, Debug)]
pub enum FunctionBlockInner {
    Axiom { sorry: bool },
    Other,
}

#[derive(Clone, Debug)]
pub struct FunctionAnnealBlock<M> {
    pub inner: FunctionBlockInner,
    _phantom: std::marker::PhantomData<M>,
}

#[derive(Clone, Debug)]
pub enum ParsedItem<M> {
    Function(AnnealDecorated<FunctionItem<M>, FunctionAnnealBlock<M>>),
    Dummy,
}

#[derive(Debug)]
pub struct ParsedLeanItem<M> {
    pub item: ParsedItem<M>,
    pub module_path: Vec<String>,
    pub source_file: std::path::PathBuf,
}

pub mod attr {
    pub use super::FunctionBlockInner;
}
