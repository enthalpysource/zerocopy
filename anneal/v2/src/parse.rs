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
